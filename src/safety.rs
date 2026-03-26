use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::Regex;

pub const DESTRUCTIVE_WARNING: &str = "# WARNING: destructive command";

static DESTRUCTIVE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?im)\b(rm\s+-rf|drop\s+table|git\s+push\b.*--force|terraform\s+destroy|kubectl\s+delete|docker\s+system\s+prune)\b")
        .expect("valid destructive pattern regex")
});

pub fn apply_warning(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.starts_with(DESTRUCTIVE_WARNING) || !DESTRUCTIVE_PATTERN.is_match(trimmed) {
        return trimmed.to_string();
    }

    format!("{DESTRUCTIVE_WARNING}\n{trimmed}")
}

pub fn normalize_expand_output(output: &str) -> Result<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        bail!("model returned an empty command");
    }

    let mut lines = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let mut warning = None;
    let mut body = Vec::new();

    if let Some(first_line) = lines.next() {
        if first_line == DESTRUCTIVE_WARNING {
            warning = Some(first_line.to_string());
        } else {
            body.push(normalize_segment(first_line));
        }
    }

    body.extend(lines.map(normalize_segment));

    let command = body
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if command.is_empty() {
        bail!("model returned a warning without a command");
    }

    if let Some(warning) = warning {
        Ok(format!("{warning}\n{command}"))
    } else {
        Ok(command)
    }
}

pub fn preview_expand_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut lines = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let mut warning = false;
    let mut body = Vec::new();

    if let Some(first_line) = lines.next() {
        if first_line == DESTRUCTIVE_WARNING {
            warning = true;
        } else {
            body.push(normalize_segment(first_line));
        }
    }

    body.extend(lines.map(normalize_segment));

    let command = body
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    match (warning, command.is_empty()) {
        (true, true) => DESTRUCTIVE_WARNING.to_string(),
        (true, false) => format!("{DESTRUCTIVE_WARNING} {command}"),
        (false, _) => command,
    }
}

pub fn split_warning(output: &str) -> (Option<String>, String) {
    let prefix = format!("{DESTRUCTIVE_WARNING}\n");
    if let Some(command) = output.strip_prefix(&prefix) {
        (
            Some("WARNING: destructive command".to_string()),
            command.to_string(),
        )
    } else {
        (None, output.trim().to_string())
    }
}

pub fn normalize_explanation(explanation: &str) -> Result<String> {
    let normalized = explanation
        .lines()
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ");

    if normalized.is_empty() {
        bail!("model returned an empty explanation");
    }

    Ok(normalized)
}

fn normalize_segment(segment: &str) -> String {
    segment.replace('\t', " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        DESTRUCTIVE_WARNING, apply_warning, normalize_expand_output, normalize_explanation,
        preview_expand_output, split_warning,
    };

    #[test]
    fn prefixes_destructive_commands() {
        let output = apply_warning("rm -rf tmp");
        assert!(output.starts_with("# WARNING: destructive command"));
    }

    #[test]
    fn keeps_single_line_command_unchanged() {
        let output = normalize_expand_output("git status").unwrap();
        assert_eq!(output, "git status");
    }

    #[test]
    fn joins_multi_line_commands() {
        let output =
            normalize_expand_output("git add .\ngit commit -m \"test\"\ngit push").unwrap();
        assert_eq!(output, "git add . git commit -m \"test\" git push");
    }

    #[test]
    fn preserves_warning_and_normalizes_command_body() {
        let output = normalize_expand_output(
            "# WARNING: destructive command\n\n git add -A &&\n\tgit push origin main\n",
        )
        .unwrap();
        assert_eq!(
            output,
            "# WARNING: destructive command\ngit add -A && git push origin main"
        );
    }

    #[test]
    fn rejects_warning_without_command() {
        let error = normalize_expand_output("# WARNING: destructive command").unwrap_err();
        assert!(error.to_string().contains("warning without a command"));
    }

    #[test]
    fn previews_partial_expand_output() {
        let output = preview_expand_output(
            "# WARNING: destructive command\n\n git add -A &&\n\tgit push origin main\n",
        );
        assert_eq!(
            output,
            "# WARNING: destructive command git add -A && git push origin main"
        );
    }

    #[test]
    fn splits_warning_from_command() {
        let (warning, command) = split_warning(&format!("{DESTRUCTIVE_WARNING}\ngit push --force"));
        assert_eq!(warning.as_deref(), Some("WARNING: destructive command"));
        assert_eq!(command, "git push --force");
    }

    #[test]
    fn normalizes_explanation_to_single_line() {
        let explanation =
            normalize_explanation("Shows\n  the\tcurrent repository status.").unwrap();
        assert_eq!(explanation, "Shows the current repository status.");
    }
}
