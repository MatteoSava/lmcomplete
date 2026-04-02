use std::path::Path;
use std::{io, io::Write};

use anyhow::Result;

use crate::cli::{ActiveStreamFormat, AuditMode, ExpandArgs};
use crate::config::{Config, ExplainDisplay};
use crate::context::RequestContext;
use crate::prompt::builder;
use crate::provider::{
    self, CompletionEventHandler, CompletionRequest, StructuredExpandRequest,
    StructuredExpandResponse,
};
use crate::safety;
use crate::stats::UsageStats;

pub async fn run(args: ExpandArgs, config_path: Option<&Path>) -> Result<()> {
    let config = Config::load(config_path)?;
    let history_limit = args.history.unwrap_or(config.history.max_entries);

    let context = RequestContext::collect(args.shell, history_limit)?;
    let shell = context.shell;
    let os = context.os.clone();
    let structured_prompt = builder::build(
        AuditMode::Expand,
        &args.query,
        context,
        config.expand.response_mode,
    );
    let provider = provider::build(&config)?;

    match args.stream_format.resolve(config.streaming.enabled) {
        ActiveStreamFormat::Off => {
            let response = provider
                .expand(StructuredExpandRequest {
                    system_prompt: structured_prompt.system_prompt,
                    user_prompt: structured_prompt.user_prompt,
                })
                .await?;
            UsageStats::record(&response.usage)?;
            println!("{}", finalize_expand_command(&response.command)?);
            Ok(())
        }
        ActiveStreamFormat::Tty => {
            let request = CompletionRequest {
                system_prompt: build_tty_system_prompt(shell.to_string(), os),
                user_prompt: structured_prompt.user_prompt,
            };
            let mut renderer = TtyExpandRenderer::default();
            match provider.stream(request, &mut renderer).await {
                Ok(response) => {
                    UsageStats::record(&response.usage)?;
                    renderer.finish_success(&finalize_expand_command(&response.content)?)?;
                    Ok(())
                }
                Err(error) => {
                    renderer.finish_error()?;
                    Err(error)
                }
            }
        }
        ActiveStreamFormat::Widget => {
            let result = provider
                .expand(StructuredExpandRequest {
                    system_prompt: structured_prompt.system_prompt,
                    user_prompt: structured_prompt.user_prompt,
                })
                .await;

            match result {
                Ok(response) => {
                    UsageStats::record(&response.usage)?;
                    WidgetExpandRenderer
                        .finish_success(&response, config.expand.explain_display)?;
                    Ok(())
                }
                Err(error) => {
                    WidgetExpandRenderer.finish_error(&error.to_string())?;
                    Err(error)
                }
            }
        }
    }
}

fn build_tty_system_prompt(shell: String, os: String) -> String {
    format!(
        "You are a shell command generator. Given a natural language description,\nreturn ONLY the shell command. No explanation, no markdown, no backticks.\nReturn exactly one shell command on one line.\nIf multiple operations are needed, chain them on one line with shell operators such as &&, ;, or |.\nShell: {shell} | OS: {os}"
    )
}

fn finalize_expand_command(content: &str) -> Result<String> {
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
    fn finish_error(&self, message: &str) -> Result<()> {
        emit_widget_event(
            "done",
            &[
                ("status", "error".to_string()),
                ("message", sanitize_widget_field(message)),
            ],
        )
    }

    fn finish_success(
        &self,
        response: &StructuredExpandResponse,
        explain_display: ExplainDisplay,
    ) -> Result<()> {
        let final_output = finalize_expand_command(&response.command)?;
        let (warning, command) = split_widget_expand_output(&final_output);
        let explanation =
            sanitize_widget_field(&safety::normalize_explanation(&response.explanation)?);
        emit_widget_event(
            "done",
            &[
                ("status", "ok".to_string()),
                ("warning", warning.to_string()),
                ("command", command),
                ("explanation", explanation),
                (
                    "display",
                    sanitize_widget_field(explain_display.as_widget_value()),
                ),
            ],
        )
    }
}

fn split_widget_expand_output(final_output: &str) -> (&'static str, String) {
    let (warning, command) = safety::split_warning(final_output);
    if warning.is_some() {
        ("warning", sanitize_widget_field(&command))
    } else {
        ("none", sanitize_widget_field(&command))
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

impl ExplainDisplay {
    fn as_widget_value(self) -> &'static str {
        match self {
            ExplainDisplay::Both => "both",
            ExplainDisplay::Inline => "inline",
            ExplainDisplay::Message => "message",
            ExplainDisplay::Off => "off",
        }
    }
}
