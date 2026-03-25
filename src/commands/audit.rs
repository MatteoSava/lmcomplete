use std::path::Path;

use anyhow::Result;

use crate::cli::AuditArgs;
use crate::config::Config;
use crate::context::RequestContext;
use crate::prompt::builder;

pub fn run(args: AuditArgs, config_path: Option<&Path>) -> Result<()> {
    let config = Config::load(config_path)?;
    let history_limit = args.history.unwrap_or(config.history.max_entries);

    let context = RequestContext::collect(args.shell, history_limit)?;
    let prompt = builder::build(args.mode, &args.input, context);

    if !prompt.warnings.is_empty() {
        println!("Warnings:");
        for warning in prompt.warnings {
            println!("  - {warning}");
        }
        println!();
    }

    println!("System prompt:\n{}\n", prompt.system_prompt);
    println!("User prompt:\n{}", prompt.user_prompt);
    Ok(())
}
