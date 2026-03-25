pub mod cwd;
pub mod history;
pub mod shell;

use anyhow::Result;

use crate::redaction::{redact, redact_lines};

use self::cwd::{CwdContext, GitContext};
use self::shell::Shell;

#[derive(Debug, Clone)]
pub struct RequestContext {
    pub shell: Shell,
    pub os: String,
    pub cwd: CwdContext,
    pub history: Vec<String>,
    pub input_warnings: Vec<String>,
}

impl RequestContext {
    pub fn collect(shell_override: Option<Shell>, history_limit: usize) -> Result<Self> {
        let shell = shell_override.unwrap_or_else(Shell::detect);
        let os = std::env::consts::OS.to_string();

        let mut cwd = cwd::collect(std::env::current_dir()?)?;
        cwd.projects = redact_lines(&cwd.projects);
        cwd.details = redact_lines(&cwd.details);
        cwd.git = cwd.git.map(|git| GitContext {
            branch: git.branch.map(|value| redact(&value).sanitized),
            status: redact_lines(&git.status),
            remotes: redact_lines(&git.remotes),
        });

        let history = redact_lines(&history::recent_commands(shell, history_limit)?);

        Ok(Self {
            shell,
            os,
            cwd,
            history,
            input_warnings: Vec::new(),
        })
    }

    pub fn with_input_warnings(mut self, warnings: Vec<String>) -> Self {
        self.input_warnings = warnings;
        self
    }
}
