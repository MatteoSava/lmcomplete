use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::context::shell::Shell;

#[derive(Debug, Parser)]
#[command(name = "lmc", version, about = "Context-aware shell command expansion")]
pub struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Expand(ExpandArgs),
    Explain(ExplainArgs),
    Audit(AuditArgs),
    Init(InitArgs),
    Stats,
}

#[derive(Debug, Clone, Args)]
pub struct ExpandArgs {
    #[arg(value_name = "QUERY")]
    pub query: String,
    #[arg(long)]
    pub shell: Option<Shell>,
    #[arg(long, value_name = "N")]
    pub history: Option<usize>,
    #[arg(long, value_enum, hide = true, default_value = "auto")]
    pub stream_format: StreamFormat,
}

#[derive(Debug, Clone, Args)]
pub struct ExplainArgs {
    #[arg(value_name = "COMMAND")]
    pub command: String,
    #[arg(long)]
    pub shell: Option<Shell>,
    #[arg(long, value_name = "N")]
    pub history: Option<usize>,
    #[arg(long, value_enum, hide = true, default_value = "auto")]
    pub stream_format: StreamFormat,
}

#[derive(Debug, Clone, Args)]
pub struct AuditArgs {
    #[arg(value_name = "INPUT")]
    pub input: String,
    #[arg(long, default_value = "expand")]
    pub mode: AuditMode,
    #[arg(long)]
    pub shell: Option<Shell>,
    #[arg(long, value_name = "N")]
    pub history: Option<usize>,
}

#[derive(Debug, Clone, Args)]
pub struct InitArgs {
    #[arg(value_enum)]
    pub shell: InitShell,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum AuditMode {
    Expand,
    Explain,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum InitShell {
    Zsh,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum StreamFormat {
    Auto,
    Off,
    Tty,
    Widget,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ActiveStreamFormat {
    Off,
    Tty,
    Widget,
}

impl StreamFormat {
    pub fn resolve(self, streaming_enabled: bool) -> ActiveStreamFormat {
        if !streaming_enabled {
            return ActiveStreamFormat::Off;
        }

        match self {
            Self::Auto => {
                if std::io::stdout().is_terminal() {
                    ActiveStreamFormat::Tty
                } else {
                    ActiveStreamFormat::Off
                }
            }
            Self::Off => ActiveStreamFormat::Off,
            Self::Tty => ActiveStreamFormat::Tty,
            Self::Widget => ActiveStreamFormat::Widget,
        }
    }
}

pub fn parse() -> Cli {
    Cli::parse_from(normalize_args(std::env::args_os().collect()))
}

fn normalize_args(mut args: Vec<OsString>) -> Vec<OsString> {
    let Some(command_index) = first_non_global_arg_index(&args) else {
        return args;
    };

    let token = args[command_index].to_string_lossy();
    if is_known_subcommand(&token) || is_passthrough_flag(&token) {
        return args;
    }

    args.insert(command_index, OsString::from("expand"));
    args
}

fn first_non_global_arg_index(args: &[OsString]) -> Option<usize> {
    let mut index = 1;

    while index < args.len() {
        let token = args[index].to_string_lossy();
        match token.as_ref() {
            "--config" => index += 2,
            _ => return Some(index),
        }
    }

    None
}

fn is_known_subcommand(token: &str) -> bool {
    matches!(token, "expand" | "explain" | "audit" | "init" | "stats")
}

fn is_passthrough_flag(token: &str) -> bool {
    matches!(token, "-h" | "--help" | "-V" | "--version")
}

#[cfg(test)]
mod tests {
    use super::normalize_args;
    use super::{ActiveStreamFormat, StreamFormat};
    use std::ffi::OsString;

    fn as_strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn injects_expand_for_plain_query() {
        let args = vec!["lmc".into(), "commit this file".into()];
        let normalized = normalize_args(args);
        assert_eq!(
            as_strings(&normalized),
            vec!["lmc", "expand", "commit this file"]
        );
    }

    #[test]
    fn preserves_explicit_subcommand() {
        let args = vec!["lmc".into(), "audit".into(), "commit this file".into()];
        let normalized = normalize_args(args);
        assert_eq!(
            as_strings(&normalized),
            vec!["lmc", "audit", "commit this file"]
        );
    }

    #[test]
    fn injects_expand_after_global_config() {
        let args = vec![
            "lmc".into(),
            "--config".into(),
            "/tmp/config.toml".into(),
            "commit this file".into(),
        ];
        let normalized = normalize_args(args);
        assert_eq!(
            as_strings(&normalized),
            vec![
                "lmc",
                "--config",
                "/tmp/config.toml",
                "expand",
                "commit this file"
            ]
        );
    }

    #[test]
    fn disables_streaming_globally() {
        assert_eq!(StreamFormat::Tty.resolve(false), ActiveStreamFormat::Off);
        assert_eq!(StreamFormat::Widget.resolve(false), ActiveStreamFormat::Off);
    }

    #[test]
    fn widget_streaming_resolves_without_tty() {
        assert_eq!(
            StreamFormat::Widget.resolve(true),
            ActiveStreamFormat::Widget
        );
    }
}
