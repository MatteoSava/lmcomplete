use std::path::Path;
use std::{io, io::Write};

use anyhow::Result;

use crate::cli::{ActiveStreamFormat, AuditMode, ExpandArgs};
use crate::config::Config;
use crate::context::RequestContext;
use crate::prompt::builder;
use crate::provider::{self, CompletionEventHandler, CompletionRequest};
use crate::safety;
use crate::stats::UsageStats;

pub async fn run(args: ExpandArgs, config_path: Option<&Path>) -> Result<()> {
    let config = Config::load(config_path)?;
    let history_limit = args.history.unwrap_or(config.history.max_entries);

    let context = RequestContext::collect(args.shell, history_limit)?;
    let prompt = builder::build(AuditMode::Expand, &args.query, context);
    let provider = provider::build(&config)?;
    let request = CompletionRequest {
        system_prompt: prompt.system_prompt,
        user_prompt: prompt.user_prompt,
    };

    match args.stream_format.resolve(config.streaming.enabled) {
        ActiveStreamFormat::Off => {
            let response = provider.complete(request).await?;
            UsageStats::record(&response.usage)?;
            println!("{}", finalize_expand_output(&response.content)?);
            Ok(())
        }
        ActiveStreamFormat::Tty => {
            let mut renderer = TtyExpandRenderer::default();
            match provider.stream(request, &mut renderer).await {
                Ok(response) => {
                    UsageStats::record(&response.usage)?;
                    renderer.finish_success(&finalize_expand_output(&response.content)?)?;
                    Ok(())
                }
                Err(error) => {
                    renderer.finish_error()?;
                    Err(error)
                }
            }
        }
        ActiveStreamFormat::Widget => {
            let mut renderer = WidgetExpandRenderer;
            match provider.stream(request, &mut renderer).await {
                Ok(response) => {
                    UsageStats::record(&response.usage)?;
                    renderer.finish_success(&finalize_expand_output(&response.content)?)?;
                    Ok(())
                }
                Err(error) => {
                    renderer.finish_error(&error)?;
                    Err(error)
                }
            }
        }
    }
}

fn finalize_expand_output(content: &str) -> Result<String> {
    let normalized = safety::normalize_expand_output(content)?;
    Ok(safety::apply_warning(&normalized))
}

#[derive(Default)]
struct TtyExpandRenderer {
    raw: String,
    last_preview: String,
}

impl TtyExpandRenderer {
    fn finish_success(&self, final_output: &str) -> Result<()> {
        let mut stdout = io::stdout();
        if !self.last_preview.is_empty() {
            write!(stdout, "\r\x1b[2K")?;
        }
        writeln!(stdout, "{final_output}")?;
        stdout.flush()?;
        Ok(())
    }

    fn finish_error(&self) -> Result<()> {
        if self.last_preview.is_empty() {
            return Ok(());
        }

        let mut stdout = io::stdout();
        write!(stdout, "\r\x1b[2K\n")?;
        stdout.flush()?;
        Ok(())
    }
}

impl CompletionEventHandler for TtyExpandRenderer {
    fn on_content(&mut self, content: &str) -> Result<()> {
        self.raw.push_str(content);
        let preview = safety::preview_expand_output(&self.raw);
        if preview.is_empty() || preview == self.last_preview {
            return Ok(());
        }

        let mut stdout = io::stdout();
        write!(stdout, "\r\x1b[2K{preview}")?;
        stdout.flush()?;
        self.last_preview = preview;
        Ok(())
    }
}

#[derive(Default)]
struct WidgetExpandRenderer;

impl WidgetExpandRenderer {
    fn finish_success(&self, final_output: &str) -> Result<()> {
        let (warning, command) = split_widget_expand_output(final_output);
        emit_widget_event(
            "done",
            &[
                ("status", "ok".to_string()),
                ("warning", warning.to_string()),
                ("command", command),
            ],
        )
    }

    fn finish_error(&self, error: &anyhow::Error) -> Result<()> {
        emit_widget_event(
            "done",
            &[
                ("status", "error".to_string()),
                ("message", sanitize_widget_field(&error.to_string())),
            ],
        )
    }
}

impl CompletionEventHandler for WidgetExpandRenderer {
    fn on_content(&mut self, _content: &str) -> Result<()> {
        Ok(())
    }
}

fn split_widget_expand_output(final_output: &str) -> (&'static str, String) {
    let warning_prefix = "# WARNING: destructive command\n";
    if let Some(command) = final_output.strip_prefix(warning_prefix) {
        ("warning", sanitize_widget_field(command))
    } else {
        ("none", sanitize_widget_field(final_output))
    }
}

fn emit_widget_event(event: &str, fields: &[(&str, String)]) -> Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "{event}")?;
    for (key, value) in fields {
        write!(stdout, "\t{key}={value}")?;
    }
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}

fn sanitize_widget_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\n' | '\r' | '\t' => ' ',
            _ => ch,
        })
        .collect::<String>()
        .trim()
        .to_string()
}
