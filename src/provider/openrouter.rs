use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde::Serialize;

use crate::config::Config;

use super::{CompletionEventHandler, CompletionRequest, CompletionResponse, Provider, Usage};

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

    async fn complete_with_model(
        &self,
        model: &str,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse> {
        let body = build_openrouter_request(model, request, None);

        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("failed to send request to OpenRouter")?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .context("failed to read OpenRouter response body")?;

        if !status.is_success() {
            bail!("OpenRouter returned {status}: {raw}");
        }

        let parsed: OpenRouterResponse =
            serde_json::from_str(&raw).context("failed to parse OpenRouter response")?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("OpenRouter returned an empty completion"))?;

        Ok(CompletionResponse {
            content,
            usage: parsed.usage.unwrap_or_default().into(),
        })
    }

    async fn stream_with_model(
        &self,
        model: &str,
        request: &CompletionRequest,
        handler: &mut dyn CompletionEventHandler,
    ) -> std::result::Result<CompletionResponse, StreamAttemptError> {
        let body = build_openrouter_request(model, request, Some(true));

        let mut response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("failed to send request to OpenRouter")
            .map_err(StreamAttemptError::pre_stream)?;

        let status = response.status();
        if !status.is_success() {
            let raw = response
                .text()
                .await
                .context("failed to read OpenRouter response body")
                .map_err(StreamAttemptError::pre_stream)?;
            return Err(StreamAttemptError::pre_stream(anyhow!(
                "OpenRouter returned {status}: {raw}"
            )));
        }

        let mut parser = SseParser::default();
        let mut content = String::new();
        let mut usage = Usage::default();
        let mut started = false;

        while let Some(chunk) = response
            .chunk()
            .await
            .context("failed to read OpenRouter stream chunk")
            .map_err(|error| StreamAttemptError::new(error, started))?
        {
            let events = parser
                .push(&chunk)
                .map_err(|error| StreamAttemptError::new(error, started))?;

            for data in events {
                if data == "[DONE]" {
                    continue;
                }

                let parsed: OpenRouterStreamResponse = serde_json::from_str(&data)
                    .context("failed to parse OpenRouter stream chunk")
                    .map_err(|error| StreamAttemptError::new(error, started))?;

                if let Some(error) = parsed.error {
                    return Err(StreamAttemptError::new(
                        anyhow!("OpenRouter stream error: {}", error.message),
                        started,
                    ));
                }

                if let Some(delta) = parsed
                    .choices
                    .into_iter()
                    .next()
                    .and_then(|choice| choice.delta)
                    .and_then(|delta| delta.content)
                    .filter(|value| !value.is_empty())
                {
                    started = true;
                    content.push_str(&delta);
                    handler
                        .on_content(&delta)
                        .map_err(|error| StreamAttemptError::new(error, true))?;
                }

                if let Some(event_usage) = parsed.usage {
                    usage = event_usage.into();
                }
            }
        }

        parser
            .finish()
            .map_err(|error| StreamAttemptError::new(error, started))?;

        if content.trim().is_empty() {
            return Err(StreamAttemptError::new(
                anyhow!("OpenRouter returned an empty completion"),
                started,
            ));
        }

        Ok(CompletionResponse { content, usage })
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        match self.complete_with_model(&self.model, &request).await {
            Ok(response) => Ok(response),
            Err(primary_error) => {
                let Some(fallback_model) = &self.fallback_model else {
                    return Err(primary_error);
                };
                if !should_use_fallback_model(&self.model, false) {
                    return Err(primary_error);
                }

                self.complete_with_model(fallback_model, &request)
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

    async fn stream(
        &self,
        request: CompletionRequest,
        handler: &mut dyn CompletionEventHandler,
    ) -> Result<CompletionResponse> {
        match self.stream_with_model(&self.model, &request, handler).await {
            Ok(response) => Ok(response),
            Err(primary_error) => {
                let Some(fallback_model) = &self.fallback_model else {
                    return Err(primary_error.error);
                };

                if !should_use_fallback_model(&self.model, primary_error.started) {
                    return Err(primary_error.error);
                }

                self.stream_with_model(fallback_model, &request, handler)
                    .await
                    .map_err(|fallback_error| {
                        anyhow!(
                            "primary model '{}' failed and fallback model '{}' also failed: {:#}; fallback error: {:#}",
                            self.model,
                            fallback_model,
                            primary_error.error,
                            fallback_error.error
                        )
                    })
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<OpenRouterProviderPreferences>,
}

#[derive(Debug, Clone, Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    choices: Vec<OpenRouterChoice>,
    usage: Option<OpenRouterUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterMessage,
}

#[derive(Debug, Deserialize)]
struct OpenRouterMessage {
    content: String,
}

#[derive(Debug, Default, Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    cost: Option<f64>,
}

#[derive(Debug, Serialize)]
struct OpenRouterProviderPreferences {
    only: Vec<String>,
    allow_fallbacks: bool,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamResponse {
    choices: Vec<OpenRouterStreamChoice>,
    usage: Option<OpenRouterUsage>,
    error: Option<OpenRouterStreamError>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChoice {
    delta: Option<OpenRouterStreamDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamError {
    message: String,
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

#[derive(Debug)]
struct StreamAttemptError {
    error: anyhow::Error,
    started: bool,
}

#[derive(Debug)]
struct RoutedModel {
    model: String,
    provider: Option<OpenRouterProviderPreferences>,
}

impl RoutedModel {
    fn new(model: &str) -> Self {
        let Some((base_model, provider_slug)) = split_provider_suffix(model) else {
            return Self {
                model: model.to_string(),
                provider: None,
            };
        };

        Self {
            model: base_model.to_string(),
            provider: Some(OpenRouterProviderPreferences {
                only: vec![provider_slug.to_string()],
                allow_fallbacks: false,
            }),
        }
    }
}

fn split_provider_suffix(model: &str) -> Option<(&str, &str)> {
    let (base_model, suffix) = model.rsplit_once(':')?;
    if matches!(suffix, "nitro" | "floor") {
        return None;
    }
    if suffix.is_empty() || base_model.is_empty() {
        return None;
    }

    Some((base_model, suffix))
}

fn should_use_fallback_model(primary_model: &str, started_streaming: bool) -> bool {
    if started_streaming {
        return false;
    }

    split_provider_suffix(primary_model).is_none()
}

fn build_openrouter_request(
    model: &str,
    request: &CompletionRequest,
    stream: Option<bool>,
) -> OpenRouterRequest {
    let routing = RoutedModel::new(model);
    OpenRouterRequest {
        model: routing.model,
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
        temperature: Some(0.0),
        stream,
        provider: routing.provider,
    }
}

impl StreamAttemptError {
    fn new(error: anyhow::Error, started: bool) -> Self {
        Self { error, started }
    }

    fn pre_stream(error: anyhow::Error) -> Self {
        Self::new(error, false)
    }
}

#[derive(Debug, Default)]
struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some((end, delimiter_len)) = find_event_boundary(&self.buffer) {
            let event = self.buffer.drain(..end + delimiter_len).collect::<Vec<_>>();
            let content = &event[..end];
            if let Some(data) = parse_sse_event(content)? {
                events.push(data);
            }
        }

        Ok(events)
    }

    fn finish(&self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let trailing = String::from_utf8_lossy(&self.buffer);
        if trailing.trim().is_empty() {
            Ok(())
        } else {
            bail!("OpenRouter stream ended with an incomplete SSE event")
        }
    }
}

fn find_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;
    while index < buffer.len() {
        if index + 1 < buffer.len() && buffer[index] == b'\n' && buffer[index + 1] == b'\n' {
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

        index += 1;
    }

    None
}

fn parse_sse_event(event: &[u8]) -> Result<Option<String>> {
    let text = std::str::from_utf8(event).context("OpenRouter SSE event was not valid UTF-8")?;
    let mut data = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }

        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.trim_start().to_string());
        }
    }

    if data.is_empty() {
        Ok(None)
    } else {
        Ok(Some(data.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionRequest, OpenRouterStreamResponse, OpenRouterUsage, RoutedModel, SseParser,
        build_openrouter_request, find_event_boundary, parse_sse_event, should_use_fallback_model,
        split_provider_suffix,
    };
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
    fn parses_sse_comments_and_content_chunks() {
        let mut parser = SseParser::default();
        let mut events = parser.push(b": OPENROUTER PROCESSING\n\n").unwrap();
        assert!(events.is_empty());

        events.extend(
            parser
                .push(
                    br#"data: {"choices":[{"delta":{"content":"git "}}]}

data: {"choices":[{"delta":{"content":"status"}}]}

"#,
                )
                .unwrap(),
        );

        assert_eq!(events.len(), 2);
        let first: OpenRouterStreamResponse = serde_json::from_str(&events[0]).unwrap();
        let second: OpenRouterStreamResponse = serde_json::from_str(&events[1]).unwrap();
        assert_eq!(
            first.choices[0].delta.as_ref().unwrap().content.as_deref(),
            Some("git ")
        );
        assert_eq!(
            second.choices[0].delta.as_ref().unwrap().content.as_deref(),
            Some("status")
        );
    }

    #[test]
    fn captures_usage_in_final_stream_chunk() {
        let event = parse_sse_event(
            br#"data: {"choices":[{"delta":{"content":""}}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"cost":0.5}}
"#,
        )
        .unwrap()
        .unwrap();

        let parsed: OpenRouterStreamResponse = serde_json::from_str(&event).unwrap();
        let usage = Usage::from(parsed.usage.unwrap());
        assert_eq!(usage.prompt_tokens, Some(1));
        assert_eq!(usage.completion_tokens, Some(2));
        assert_eq!(usage.total_tokens, Some(3));
        assert_eq!(usage.cost, Some(0.5));
    }

    #[test]
    fn detects_mixed_newline_boundaries() {
        assert_eq!(find_event_boundary(b"data: 1\n\ndata: 2"), Some((7, 2)));
        assert_eq!(find_event_boundary(b"data: 1\r\n\r\ndata: 2"), Some((7, 4)));
    }

    #[test]
    fn joins_multiline_sse_data_fields() {
        let event = parse_sse_event(b"data: first\ndata: second\n\n")
            .unwrap()
            .unwrap();
        assert_eq!(event, "first\nsecond");
    }

    #[test]
    fn routes_provider_suffixes_via_provider_preferences() {
        let routed = RoutedModel::new("openai/gpt-oss-120b:groq");
        assert_eq!(routed.model, "openai/gpt-oss-120b");
        let provider = routed.provider.unwrap();
        assert_eq!(provider.only, vec!["groq"]);
        assert!(!provider.allow_fallbacks);
    }

    #[test]
    fn preserves_openrouter_shortcuts() {
        assert_eq!(split_provider_suffix("openai/gpt-oss-120b:nitro"), None);
        assert_eq!(split_provider_suffix("openai/gpt-oss-120b:floor"), None);
    }

    #[test]
    fn disables_model_fallback_for_provider_pinned_models() {
        assert!(!should_use_fallback_model(
            "openai/gpt-oss-120b:groq",
            false
        ));
        assert!(!should_use_fallback_model("openai/gpt-oss-120b", true));
        assert!(should_use_fallback_model("openai/gpt-oss-120b", false));
    }

    #[test]
    fn uses_zero_temperature() {
        let request = build_openrouter_request(
            "openai/gpt-oss-120b:groq",
            &CompletionRequest {
                system_prompt: "system".into(),
                user_prompt: "user".into(),
            },
            Some(true),
        );

        assert_eq!(request.temperature, Some(0.0));
    }
}
