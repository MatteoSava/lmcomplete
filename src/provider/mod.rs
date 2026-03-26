mod ollama;
mod openrouter;

use anyhow::Result;
use async_trait::async_trait;

use crate::config::Config;

pub use ollama::OllamaProvider;
pub use openrouter::OpenRouterProvider;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub system_prompt: String,
    pub user_prompt: String,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionResponse {
    pub content: String,
    pub usage: Usage,
}

pub type ChunkCallback<'a> = dyn FnMut(&str) -> Result<()> + Send + 'a;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete_stream(
        &self,
        request: CompletionRequest,
        on_chunk: &mut ChunkCallback<'_>,
    ) -> Result<CompletionResponse>;

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let mut streamed = String::new();
        let response = self
            .complete_stream(request, &mut |chunk| {
                streamed.push_str(chunk);
                Ok(())
            })
            .await?;
        debug_assert_eq!(response.content, streamed);
        Ok(response)
    }
}

pub fn build(config: &Config) -> Result<Box<dyn Provider>> {
    config.require_provider_config()?;
    match config.provider.name.as_str() {
        "ollama" => Ok(Box::new(OllamaProvider::new(config.clone())?)),
        "openrouter" => Ok(Box::new(OpenRouterProvider::new(config.clone())?)),
        other => Err(anyhow::anyhow!("unsupported provider '{other}'")),
    }
}
