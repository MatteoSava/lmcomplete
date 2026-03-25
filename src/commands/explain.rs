use std::path::Path;

use anyhow::Result;

use crate::cli::{AuditMode, ExplainArgs};
use crate::config::Config;
use crate::context::RequestContext;
use crate::prompt::builder;
use crate::provider::{self, CompletionRequest};
use crate::stats::UsageStats;

pub async fn run(args: ExplainArgs, config_path: Option<&Path>) -> Result<()> {
    let config = Config::load(config_path)?;
    let history_limit = args.history.unwrap_or(config.history.max_entries);

    let context = RequestContext::collect(args.shell, history_limit)?;
    let prompt = builder::build(AuditMode::Explain, &args.command, context);
    let provider = provider::build(&config)?;
    let response = provider
        .complete(CompletionRequest {
            system_prompt: prompt.system_prompt,
            user_prompt: prompt.user_prompt,
        })
        .await?;
    UsageStats::record(&response.usage)?;
    println!("{}", response.content.trim());
    Ok(())
}
