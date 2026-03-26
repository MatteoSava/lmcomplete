mod openrouter;

use anyhow::Result;
use async_trait::async_trait;

use crate::config::Config;

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

pub trait CompletionEventHandler: Send {
    fn on_content(&mut self, content: &str) -> Result<()>;
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(
        &self,
        request: CompletionRequest,
        handler: &mut dyn CompletionEventHandler,
    ) -> Result<CompletionResponse>;
}

pub fn build(config: &Config) -> Result<Box<dyn Provider>> {
    config.require_provider_config()?;
    match config.provider.name.as_str() {
        "openrouter" => Ok(Box::new(OpenRouterProvider::new(config.clone())?)),
        other => Err(anyhow::anyhow!("unsupported provider '{other}'")),
    }
}
