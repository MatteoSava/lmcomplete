use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone)]
pub struct Redaction {
    pub sanitized: String,
    pub findings: Vec<String>,
}

static SECRET_PATTERNS: LazyLock<HashMap<String, Regex>> = LazyLock::new(|| {
    let mut patterns = secretscan::patterns::get_all_patterns_owned();
    patterns.insert(
        "export_key".to_string(),
        Regex::new(r"(?m)export\s+\w*KEY\w*=\S+").expect("valid export key regex"),
    );
    patterns.insert(
        "export_secret".to_string(),
        Regex::new(r"(?m)export\s+\w*SECRET\w*=\S+").expect("valid export secret regex"),
    );
    patterns.insert(
        "export_token".to_string(),
        Regex::new(r"(?m)export\s+\w*TOKEN\w*=\S+").expect("valid export token regex"),
    );
    patterns.insert(
        "export_password".to_string(),
        Regex::new(r"(?m)export\s+\w*PASSWORD\w*=\S+").expect("valid export password regex"),
    );
    patterns.insert(
        "authorization_header".to_string(),
        Regex::new(r#"(?i)-H\s+['"]Authorization:[^'"]+['"]"#)
            .expect("valid authorization header regex"),
    );
    patterns
});

pub fn redact(text: &str) -> Redaction {
    let mut sanitized = text.to_string();
    let mut findings = Vec::new();

    for (name, pattern) in SECRET_PATTERNS.iter() {
        if pattern.is_match(&sanitized) {
            findings.push(name.clone());
            sanitized = pattern.replace_all(&sanitized, "[REDACTED]").into_owned();
        }
    }

    Redaction {
        sanitized,
        findings,
    }
}

pub fn redact_lines(lines: &[String]) -> Vec<String> {
    lines.iter().map(|line| redact(line).sanitized).collect()
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_exported_credentials() {
        let redaction = redact("export OPENROUTER_API_KEY=sk-secret");
        assert_eq!(redaction.sanitized, "[REDACTED]");
        assert!(!redaction.findings.is_empty());
    }

    #[test]
    fn redacts_authorization_headers() {
        let redaction = redact(r#"curl -H "Authorization: Bearer sk-secret" https://example.com"#);
        assert_eq!(redaction.sanitized, "curl [REDACTED] https://example.com");
    }
}
