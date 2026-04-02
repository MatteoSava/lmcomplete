use crate::cli::AuditMode;
use crate::config::ExpandResponseMode;
use crate::context::RequestContext;
use crate::redaction::redact;

#[derive(Debug, Clone)]
pub struct PromptBundle {
    pub system_prompt: String,
    pub user_prompt: String,
    pub warnings: Vec<String>,
}

pub fn build(
    mode: AuditMode,
    input: &str,
    context: RequestContext,
    expand_response_mode: ExpandResponseMode,
) -> PromptBundle {
    let redaction = redact(input);
    let mut warnings = context.input_warnings;
    if !redaction.findings.is_empty() {
        warnings.push("input contained secret-like values and was redacted before sending".into());
    }

    let system_prompt = match mode {
        AuditMode::Expand => {
            let response_contract = match expand_response_mode {
                ExpandResponseMode::ToolCall => {
                    "Use the provided tool exactly once to return the fields. Do not reply in plain text."
                }
                ExpandResponseMode::MessageJson => {
                    "Return ONLY a JSON object with keys \"command\" and \"explanation\". Do not include markdown, code fences, or extra text."
                }
            };

            format!(
                "You are a shell command generator. Given a natural language description,\nreturn a shell command and a concise explanation for developers.\nCommand requirements:\n- Return exactly one shell command on one line.\n- No markdown or backticks.\n- If multiple operations are needed, chain them on one line with shell operators such as &&, ;, or |.\nExplanation requirements:\n- Return one short plain-text sentence.\n- Focus on what the command does.\n{response_contract}\nShell: {} | OS: {}",
                context.shell, context.os
            )
        }
        AuditMode::Explain => format!(
            "You explain shell commands for developers.\nReturn a concise explanation in plain text with short bullet points.\nShell: {} | OS: {}",
            context.shell, context.os
        ),
    };

    let mut lines = vec![format!("Shell: {} | OS: {}", context.shell, context.os)];

    if !context.cwd.projects.is_empty() {
        lines.push(format!("Project: {}", context.cwd.projects.join(", ")));
    }

    for detail in context.cwd.details {
        lines.push(detail);
    }

    if let Some(git) = context.cwd.git {
        if let Some(branch) = git.branch {
            lines.push(format!("Git branch: {branch}"));
        }
        if !git.remotes.is_empty() {
            lines.push(format!("Git remotes: {}", git.remotes.join(", ")));
        }
        if !git.status.is_empty() {
            lines.push("Git status:".to_string());
            lines.extend(git.status.into_iter().map(|line| format!("  {line}")));
        }
    }

    if !context.history.is_empty() {
        lines.push("Recent commands:".to_string());
        lines.extend(context.history.into_iter().map(|line| format!("  {line}")));
    }

    let label = match mode {
        AuditMode::Expand => "User",
        AuditMode::Explain => "Command",
    };
    lines.push(format!("{label}: {}", redaction.sanitized));

    PromptBundle {
        system_prompt,
        user_prompt: lines.join("\n"),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::AuditMode;
    use crate::config::ExpandResponseMode;
    use crate::context::RequestContext;
    use crate::context::cwd::{CwdContext, GitContext};
    use crate::context::shell::Shell;

    use super::build;

    #[test]
    fn includes_git_and_history_context() {
        let context = RequestContext {
            shell: Shell::Zsh,
            os: "macos".into(),
            cwd: CwdContext {
                projects: vec!["git repo".into(), "rust project".into()],
                details: vec!["make targets: build, test".into()],
                git: Some(GitContext {
                    branch: Some("feat/login".into()),
                    status: vec!["M src/main.rs".into()],
                    remotes: vec!["origin -> github.com/user/app".into()],
                }),
            },
            history: vec!["cargo test".into()],
            input_warnings: Vec::new(),
        };

        let prompt = build(
            AuditMode::Expand,
            "commit all changes",
            context,
            ExpandResponseMode::ToolCall,
        );
        assert!(prompt.user_prompt.contains("Git branch: feat/login"));
        assert!(prompt.user_prompt.contains("Recent commands:"));
        assert!(prompt.user_prompt.contains("User: commit all changes"));
    }

    #[test]
    fn expand_prompt_requires_single_line_output() {
        let context = RequestContext {
            shell: Shell::Zsh,
            os: "macos".into(),
            cwd: CwdContext::default(),
            history: Vec::new(),
            input_warnings: Vec::new(),
        };

        let prompt = build(
            AuditMode::Expand,
            "show git status",
            context,
            ExpandResponseMode::ToolCall,
        );
        assert!(
            prompt
                .system_prompt
                .contains("exactly one shell command on one line")
        );
        assert!(prompt.system_prompt.contains("chain them on one line"));
        assert!(prompt.system_prompt.contains("provided tool exactly once"));
    }

    #[test]
    fn message_json_prompt_requires_json_response() {
        let context = RequestContext {
            shell: Shell::Zsh,
            os: "macos".into(),
            cwd: CwdContext::default(),
            history: Vec::new(),
            input_warnings: Vec::new(),
        };

        let prompt = build(
            AuditMode::Expand,
            "show git status",
            context,
            ExpandResponseMode::MessageJson,
        );

        assert!(prompt.system_prompt.contains("Return ONLY a JSON object"));
        assert!(
            prompt
                .system_prompt
                .contains("\"command\" and \"explanation\"")
        );
    }
}
