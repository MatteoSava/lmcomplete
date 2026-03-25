use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde::Serialize;

use crate::config::Config;

use super::{CompletionRequest, CompletionResponse, Provider, Usage};

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
        let body = OpenRouterRequest {
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
        };

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
struct OpenRouterRequest<'a> {
    model: &'a str,
    messages: Vec<Message>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
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

#[cfg(test)]
mod tests {
    use super::OpenRouterUsage;
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
}
