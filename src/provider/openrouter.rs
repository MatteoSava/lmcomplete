use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde::Serialize;

use crate::config::Config;

use super::{ChunkCallback, CompletionRequest, CompletionResponse, Provider, Usage};

pub struct OpenRouterProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    fallback_model: Option<String>,
}

impl OpenRouterProvider {
    pub fn new(config: Config) -> Result<Self> {
        let api_key = config
            .provider_api_key()
            .map(ToString::to_string)
            .ok_or_else(|| anyhow!("missing OpenRouter API key"))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))
                .context("failed to build authorization header")?,
        );
        headers.insert("X-Title", HeaderValue::from_static("lmcomplete"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            client,
            endpoint: config.provider.base_url,
            api_key,
            model: config.provider.model,
            fallback_model: config.provider.fallback.map(|value| value.model),
        })
    }

    async fn complete_stream_with_model(
        &self,
        model: &str,
        request: &CompletionRequest,
        on_chunk: &mut ChunkCallback<'_>,
    ) -> Result<CompletionResponse> {
        let body = self.build_request(model, request, true);

        let mut response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("failed to send request to OpenRouter")?;

        let status = response.status();
        if !status.is_success() {
            let raw = response
                .text()
                .await
                .context("failed to read OpenRouter response body")?;
            bail!("OpenRouter returned {status}: {raw}");
        }

        let mut content = String::new();
        let mut usage = Usage::default();
        let mut buffer = Vec::new();

        while let Some(chunk) = response
            .chunk()
            .await
            .context("failed to read OpenRouter streaming response body")?
        {
            buffer.extend_from_slice(&chunk);

            while let Some(event) = next_sse_event(&mut buffer) {
                if process_sse_event(&event, &mut content, &mut usage, on_chunk)? {
                    if content.trim().is_empty() {
                        bail!("OpenRouter returned an empty completion");
                    }

                    return Ok(CompletionResponse { content, usage });
                }
            }
        }

        while let Some(event) = take_trailing_sse_event(&mut buffer) {
            if process_sse_event(&event, &mut content, &mut usage, on_chunk)? {
                break;
            }
        }

        if content.trim().is_empty() {
            bail!("OpenRouter returned an empty completion");
        }

        Ok(CompletionResponse { content, usage })
    }

    fn build_request<'a>(
        &self,
        model: &'a str,
        request: &CompletionRequest,
        stream: bool,
    ) -> OpenRouterRequest<'a> {
        OpenRouterRequest {
            model,
            messages: vec![
                Message {
                    role: "system",
                    content: request.system_prompt.clone(),
                },
                Message {
                    role: "user",
                    content: request.user_prompt.clone(),
                },
            ],
            max_tokens: Some(256),
            temperature: Some(0.1),
            stream: if stream { Some(true) } else { None },
        }
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    async fn complete_stream(
        &self,
        request: CompletionRequest,
        on_chunk: &mut ChunkCallback<'_>,
    ) -> Result<CompletionResponse> {
        let mut primary_emitted = false;
        match self
            .complete_stream_with_model(&self.model, &request, &mut |chunk| {
                primary_emitted = true;
                on_chunk(chunk)
            })
            .await
        {
            Ok(response) => Ok(response),
            Err(primary_error) => {
                let Some(fallback_model) = &self.fallback_model else {
                    return Err(primary_error);
                };

                if primary_emitted {
                    return Err(primary_error.context(format!(
                        "primary model '{}' failed after streaming output; fallback model '{}' was not attempted",
                        self.model, fallback_model
                    )));
                }

                self.complete_stream_with_model(fallback_model, &request, on_chunk)
                    .await
                    .with_context(|| {
                        format!(
                            "primary model '{}' failed and fallback model '{}' also failed: {primary_error:#}",
                            self.model, fallback_model
                        )
                    })
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenRouterRequest<'a> {
    model: &'a str,
    messages: Vec<Message>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChunk {
    choices: Vec<OpenRouterStreamChoice>,
    usage: Option<OpenRouterUsage>,
    error: Option<OpenRouterStreamError>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChoice {
    delta: Option<OpenRouterDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamError {
    message: String,
}

#[derive(Debug, Default, Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    cost: Option<f64>,
}

impl From<OpenRouterUsage> for Usage {
    fn from(value: OpenRouterUsage) -> Self {
        Self {
            prompt_tokens: value.prompt_tokens,
            completion_tokens: value.completion_tokens,
            total_tokens: value.total_tokens,
            cost: value.cost,
        }
    }
}

fn process_sse_event(
    event: &[u8],
    content: &mut String,
    usage: &mut Usage,
    on_chunk: &mut ChunkCallback<'_>,
) -> Result<bool> {
    let text =
        std::str::from_utf8(event).context("OpenRouter returned invalid UTF-8 in SSE event")?;
    let mut data_lines = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }

        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start());
        }
    }

    if data_lines.is_empty() {
        return Ok(false);
    }

    let payload = data_lines.join("\n");
    if payload == "[DONE]" {
        return Ok(true);
    }

    let parsed: OpenRouterStreamChunk =
        serde_json::from_str(&payload).context("failed to parse OpenRouter stream chunk")?;
    if let Some(error) = parsed.error {
        bail!("OpenRouter stream error: {}", error.message);
    }

    if let Some(chunk_usage) = parsed.usage {
        *usage = chunk_usage.into();
    }

    for choice in parsed.choices {
        let Some(delta) = choice.delta else {
            continue;
        };
        let Some(chunk) = delta.content else {
            continue;
        };
        if chunk.is_empty() {
            continue;
        }
        content.push_str(&chunk);
        on_chunk(&chunk)?;
    }

    Ok(false)
}

fn next_sse_event(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let (index, separator_len) = sse_event_boundary(buffer)?;
    let event = buffer[..index].to_vec();
    buffer.drain(..index + separator_len);
    Some(event)
}

fn take_trailing_sse_event(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    if buffer.is_empty() || buffer.iter().all(u8::is_ascii_whitespace) {
        buffer.clear();
        return None;
    }

    let event = buffer.clone();
    buffer.clear();
    Some(event)
}

fn sse_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;

    while index + 1 < buffer.len() {
        if buffer[index] == b'\n' && buffer[index + 1] == b'\n' {
            return Some((index, 2));
        }

        if index + 3 < buffer.len()
            && buffer[index] == b'\r'
            && buffer[index + 1] == b'\n'
            && buffer[index + 2] == b'\r'
            && buffer[index + 3] == b'\n'
        {
            return Some((index, 4));
        }

        if buffer[index] == b'\r' && buffer[index + 1] == b'\r' {
            return Some((index, 2));
        }

        index += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{OpenRouterUsage, next_sse_event, process_sse_event};
    use crate::provider::Usage;

    #[test]
    fn converts_usage() {
        let usage = Usage::from(OpenRouterUsage {
            prompt_tokens: Some(10),
            completion_tokens: Some(20),
            total_tokens: Some(30),
            cost: Some(0.42),
        });
        assert_eq!(usage.total_tokens, Some(30));
        assert_eq!(usage.cost, Some(0.42));
    }

    #[test]
    fn extracts_sse_events() {
        let mut buffer = b"data: one\n\ndata: two\r\n\r\n".to_vec();

        let first = next_sse_event(&mut buffer).unwrap();
        let second = next_sse_event(&mut buffer).unwrap();

        assert_eq!(String::from_utf8(first).unwrap(), "data: one");
        assert_eq!(String::from_utf8(second).unwrap(), "data: two");
    }

    #[test]
    fn ignores_comments_and_collects_usage() {
        let mut content = String::new();
        let mut usage = Usage::default();
        let mut rendered = String::new();

        let done = process_sse_event(
            b": OPENROUTER PROCESSING",
            &mut content,
            &mut usage,
            &mut |chunk| {
                rendered.push_str(chunk);
                Ok(())
            },
        )
        .unwrap();
        assert!(!done);

        let done = process_sse_event(
            br#"data: {"choices":[{"delta":{"content":"hello"}}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"cost":0.1}}"#,
            &mut content,
            &mut usage,
            &mut |chunk| {
                rendered.push_str(chunk);
                Ok(())
            },
        )
        .unwrap();

        assert!(!done);
        assert_eq!(rendered, "hello");
        assert_eq!(content, "hello");
        assert_eq!(usage.total_tokens, Some(3));
    }
}
