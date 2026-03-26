#[cfg(test)]
use std::io::Read;
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::net::TcpListener;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde::Serialize;

use crate::config::Config;

use super::{CompletionRequest, CompletionResponse, Provider, Usage};

pub struct OllamaProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<String>,
    model: String,
    fallback_model: Option<String>,
}

impl OllamaProvider {
    pub fn new(config: Config) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let api_key = config.provider_api_key().map(ToString::to_string);

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
        let body = OllamaRequest {
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
            stream: false,
        };

        let mut request_builder = self.client.post(&self.endpoint).json(&body);
        if let Some(api_key) = &self.api_key {
            request_builder = request_builder.bearer_auth(api_key);
        }

        let response = request_builder
            .send()
            .await
            .context("failed to send request to Ollama")?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .context("failed to read Ollama response body")?;

        if !status.is_success() {
            bail!("Ollama returned {status}: {raw}");
        }

        let parsed: OllamaResponse =
            serde_json::from_str(&raw).context("failed to parse Ollama response")?;
        let content = parsed.message.content.trim().to_string();

        if content.is_empty() {
            return Err(anyhow!("Ollama returned an empty completion"));
        }

        Ok(CompletionResponse {
            content,
            usage: parsed.usage(),
        })
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        match self.complete_with_model(&self.model, &request).await {
            Ok(response) => Ok(response),
            Err(primary_error) => {
                let Some(fallback_model) = &self.fallback_model else {
                    return Err(primary_error);
                };

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
}

#[derive(Debug, Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
}

impl OllamaResponse {
    fn usage(&self) -> Usage {
        let total_tokens = match (self.prompt_eval_count, self.eval_count) {
            (Some(prompt_tokens), Some(completion_tokens)) => {
                Some(prompt_tokens + completion_tokens)
            }
            _ => None,
        };

        Usage {
            prompt_tokens: self.prompt_eval_count,
            completion_tokens: self.eval_count,
            total_tokens,
            cost: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
}

#[cfg(test)]
mod tests {
    use std::thread;

    use serde_json::Value;

    use super::*;
    use crate::config::{Config, HistoryConfig, ProviderConfig};

    #[tokio::test]
    async fn sends_chat_request_and_parses_response() {
        let endpoint = spawn_test_server(|request| {
            let (headers, body) = split_request(&request);
            assert!(headers.starts_with("POST /api/chat HTTP/1.1"));

            let body: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(body["model"], "qwen2.5-coder");
            assert_eq!(body["stream"], false);
            assert_eq!(body["messages"][0]["role"], "system");
            assert_eq!(body["messages"][0]["content"], "system prompt");
            assert_eq!(body["messages"][1]["role"], "user");
            assert_eq!(body["messages"][1]["content"], "user prompt");

            json_response(
                200,
                r#"{
  "message": {"content": "git status"},
  "prompt_eval_count": 12,
  "eval_count": 5
}"#,
            )
        });

        let provider = OllamaProvider::new(Config {
            provider: ProviderConfig {
                name: "ollama".to_string(),
                api_key: None,
                model: "qwen2.5-coder".to_string(),
                base_url: endpoint,
                fallback: None,
            },
            history: HistoryConfig::default(),
        })
        .unwrap();

        let response = provider
            .complete(CompletionRequest {
                system_prompt: "system prompt".to_string(),
                user_prompt: "user prompt".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(response.content, "git status");
        assert_eq!(response.usage.prompt_tokens, Some(12));
        assert_eq!(response.usage.completion_tokens, Some(5));
        assert_eq!(response.usage.total_tokens, Some(17));
        assert_eq!(response.usage.cost, None);
    }

    #[tokio::test]
    async fn sends_bearer_auth_when_api_key_is_configured() {
        let endpoint = spawn_test_server(|request| {
            let (headers, _) = split_request(&request);
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer ollama-key")
            );

            json_response(
                200,
                r#"{
  "message": {"content": "git status"}
}"#,
            )
        });

        let provider = OllamaProvider::new(Config {
            provider: ProviderConfig {
                name: "ollama".to_string(),
                api_key: Some("ollama-key".to_string()),
                model: "qwen2.5-coder".to_string(),
                base_url: endpoint,
                fallback: None,
            },
            history: HistoryConfig::default(),
        })
        .unwrap();

        let response = provider
            .complete(CompletionRequest {
                system_prompt: "system prompt".to_string(),
                user_prompt: "user prompt".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(response.content, "git status");
    }

    fn spawn_test_server(handler: impl FnOnce(Vec<u8>) -> String + Send + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            let response = handler(request);
            stream.write_all(response.as_bytes()).unwrap();
        });

        format!("http://{address}/api/chat")
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut header_end = None;

        while header_end.is_none() {
            let mut chunk = [0; 1024];
            let bytes_read = stream.read(&mut chunk).unwrap();
            request.extend_from_slice(&chunk[..bytes_read]);
            header_end = find_header_end(&request);
        }

        let header_end = header_end.unwrap();
        let content_length = parse_content_length(&request[..header_end]).unwrap_or(0);
        while request.len() < header_end + content_length {
            let mut chunk = [0; 1024];
            let bytes_read = stream.read(&mut chunk).unwrap();
            request.extend_from_slice(&chunk[..bytes_read]);
        }

        request
    }

    fn find_header_end(request: &[u8]) -> Option<usize> {
        request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
    }

    fn parse_content_length(headers: &[u8]) -> Option<usize> {
        String::from_utf8_lossy(headers).lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
    }

    fn split_request(request: &[u8]) -> (String, &[u8]) {
        let header_end = find_header_end(request).unwrap();
        (
            String::from_utf8(request[..header_end].to_vec()).unwrap(),
            &request[header_end..],
        )
    }

    fn json_response(status_code: u16, body: &str) -> String {
        let reason = match status_code {
            200 => "OK",
            _ => "ERROR",
        };

        format!(
            "HTTP/1.1 {status_code} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
    }
}
