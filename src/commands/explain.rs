use std::path::Path;
use std::{io, io::Write};

use anyhow::Result;

use crate::cli::{ActiveStreamFormat, AuditMode, ExplainArgs};
use crate::config::Config;
use crate::context::RequestContext;
use crate::prompt::builder;
use crate::provider::{self, CompletionEventHandler, CompletionRequest};
use crate::stats::UsageStats;

pub async fn run(args: ExplainArgs, config_path: Option<&Path>) -> Result<()> {
    let config = Config::load(config_path)?;
    let history_limit = args.history.unwrap_or(config.history.max_entries);

    let context = RequestContext::collect(args.shell, history_limit)?;
    let prompt = builder::build(AuditMode::Explain, &args.command, context);
    let provider = provider::build(&config)?;
    let request = CompletionRequest {
        system_prompt: prompt.system_prompt,
        user_prompt: prompt.user_prompt,
    };

    match args.stream_format.resolve(config.streaming.enabled) {
        ActiveStreamFormat::Off => {
            let response = provider.complete(request).await?;
            UsageStats::record(&response.usage)?;
            println!("{}", response.content.trim());
            Ok(())
        }
        ActiveStreamFormat::Tty | ActiveStreamFormat::Widget => {
            let mut renderer = TtyExplainRenderer::default();
            match provider.stream(request, &mut renderer).await {
                Ok(response) => {
                    UsageStats::record(&response.usage)?;
                    renderer.finish_success(&response.content)?;
                    Ok(())
                }
                Err(error) => {
                    renderer.finish_error()?;
                    Err(error)
                }
            }
        }
    }
}

#[derive(Default)]
struct TtyExplainRenderer {
    wrote_anything: bool,
}

impl TtyExplainRenderer {
    fn finish_success(&self, content: &str) -> Result<()> {
        let mut stdout = io::stdout();
        if !content.ends_with('\n') {
            writeln!(stdout)?;
        }
        stdout.flush()?;
        Ok(())
    }

    fn finish_error(&self) -> Result<()> {
        if !self.wrote_anything {
            return Ok(());
        }

        let mut stdout = io::stdout();
        writeln!(stdout)?;
        stdout.flush()?;
        Ok(())
    }
}

impl CompletionEventHandler for TtyExplainRenderer {
    fn on_content(&mut self, content: &str) -> Result<()> {
        let mut stdout = io::stdout();
        write!(stdout, "{content}")?;
        stdout.flush()?;
        self.wrote_anything = true;
        Ok(())
    }
}
