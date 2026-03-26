use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::Regex;

static DESTRUCTIVE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?im)\b(rm\s+-rf|drop\s+table|git\s+push\b.*--force|terraform\s+destroy|kubectl\s+delete|docker\s+system\s+prune)\b")
        .expect("valid destructive pattern regex")
});

pub fn apply_warning(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.starts_with("# WARNING: destructive command")
        || !DESTRUCTIVE_PATTERN.is_match(trimmed)
    {
        return trimmed.to_string();
    }

    format!("# WARNING: destructive command\n{trimmed}")
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
        if first_line == "# WARNING: destructive command" {
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
        if first_line == "# WARNING: destructive command" {
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
        (true, true) => "# WARNING: destructive command".to_string(),
        (true, false) => format!("# WARNING: destructive command {command}"),
        (false, _) => command,
    }
}

fn normalize_segment(segment: &str) -> String {
    segment.replace('\t', " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{apply_warning, normalize_expand_output, preview_expand_output};

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
}
