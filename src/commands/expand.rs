use std::io;
use std::path::Path;

use anyhow::Result;

use crate::cli::{AuditMode, ExpandArgs};
use crate::config::Config;
use crate::context::RequestContext;
use crate::output::{ExpandOutput, verify_expand_output};
use crate::prompt::builder;
use crate::provider::{self, CompletionRequest};
use crate::safety;
use crate::stats::UsageStats;

pub async fn run(args: ExpandArgs, config_path: Option<&Path>) -> Result<()> {
    let config = Config::load(config_path)?;
    let history_limit = args.history.unwrap_or(config.history.max_entries);

    let context = RequestContext::collect(args.shell, history_limit)?;
    let prompt = builder::build(AuditMode::Expand, &args.query, context);
    let provider = provider::build(&config)?;
    let mut stdout = io::stdout();
    let mut output = ExpandOutput::default();
    let response = provider
        .complete_stream(
            CompletionRequest {
                system_prompt: prompt.system_prompt,
                user_prompt: prompt.user_prompt,
            },
            &mut |chunk| output.push(chunk, &mut stdout),
        )
        .await?;
    let normalized = safety::normalize_expand_output(&response.content)?;
    let expected = safety::apply_warning(&normalized);
    let rendered = output.finish(&mut stdout)?;
    verify_expand_output(rendered, &expected)?;
    UsageStats::record(&response.usage)?;
    Ok(())
}
