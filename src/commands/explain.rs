use std::io;
use std::path::Path;

use anyhow::Result;

use crate::cli::{AuditMode, ExplainArgs};
use crate::config::Config;
use crate::context::RequestContext;
use crate::output::TrimmedOutput;
use crate::prompt::builder;
use crate::provider::{self, CompletionRequest};
use crate::stats::UsageStats;

pub async fn run(args: ExplainArgs, config_path: Option<&Path>) -> Result<()> {
    let config = Config::load(config_path)?;
    let history_limit = args.history.unwrap_or(config.history.max_entries);

    let context = RequestContext::collect(args.shell, history_limit)?;
    let prompt = builder::build(AuditMode::Explain, &args.command, context);
    let provider = provider::build(&config)?;
    let mut stdout = io::stdout();
    let mut output = TrimmedOutput::default();
    let response = provider
        .complete_stream(
            CompletionRequest {
                system_prompt: prompt.system_prompt,
                user_prompt: prompt.user_prompt,
            },
            &mut |chunk| output.push(chunk, &mut stdout),
        )
        .await?;
    output.finish(&mut stdout)?;
    UsageStats::record(&response.usage)?;
    Ok(())
}
