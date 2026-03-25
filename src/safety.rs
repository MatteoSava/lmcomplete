use std::sync::LazyLock;

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

#[cfg(test)]
mod tests {
    use super::apply_warning;

    #[test]
    fn prefixes_destructive_commands() {
        let output = apply_warning("rm -rf tmp");
        assert!(output.starts_with("# WARNING: destructive command"));
    }
}
