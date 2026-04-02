use std::fmt;
use std::path::Path;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
    Sh,
}

impl Shell {
    pub fn detect() -> Self {
        let value = std::env::var("SHELL").unwrap_or_default();
        let name = Path::new(&value)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        match name {
            "zsh" => Self::Zsh,
            "bash" => Self::Bash,
            "fish" => Self::Fish,
            "sh" => Self::Sh,
            _ => Self::Zsh,
        }
    }
}

impl fmt::Display for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::Sh => "sh",
        };
        write!(f, "{value}")
    }
}
