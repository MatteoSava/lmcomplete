use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct CwdContext {
    pub projects: Vec<String>,
    pub details: Vec<String>,
    pub git: Option<GitContext>,
}

#[derive(Debug, Clone, Default)]
pub struct GitContext {
    pub branch: Option<String>,
    pub status: Vec<String>,
    pub remotes: Vec<String>,
}

pub fn collect(base_dir: PathBuf) -> Result<CwdContext> {
    let mut context = CwdContext::default();

    if is_git_repo(&base_dir) {
        context.projects.push("git repo".to_string());
        context.git = Some(git_context(&base_dir));
    }

    if base_dir.join("Cargo.toml").exists() {
        context.projects.push("rust project".to_string());
    }

    if let Some(scripts) = package_scripts(base_dir.join("package.json")) {
        context.projects.push("node project".to_string());
        if !scripts.is_empty() {
            context
                .details
                .push(format!("package.json scripts: {}", scripts.join(", ")));
        }
    }

    if let Some(module_name) = go_module_name(base_dir.join("go.mod")) {
        context.projects.push("go project".to_string());
        context.details.push(format!("go module: {module_name}"));
    }

    if base_dir.join("Dockerfile").exists() {
        context.projects.push("docker project".to_string());
    }

    if let Some(services) = compose_services(&base_dir) {
        context.projects.push("compose project".to_string());
        if !services.is_empty() {
            context
                .details
                .push(format!("compose services: {}", services.join(", ")));
        }
    }

    if has_kubernetes_manifests(&base_dir) {
        context.projects.push("kubernetes manifests".to_string());
    }

    if let Some(targets) = make_targets(base_dir.join("Makefile")) {
        context.projects.push("make project".to_string());
        if !targets.is_empty() {
            context
                .details
                .push(format!("make targets: {}", targets.join(", ")));
        }
    }

    if base_dir.join("pyproject.toml").exists() || base_dir.join("setup.py").exists() {
        context.projects.push("python project".to_string());
    }

    if base_dir.join("Terraform").exists() || has_tf_files(&base_dir) {
        context.projects.push("terraform project".to_string());
    }

    context.projects.sort();
    context.projects.dedup();
    Ok(context)
}

fn is_git_repo(base_dir: &Path) -> bool {
    Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .current_dir(base_dir)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git_context(base_dir: &Path) -> GitContext {
    GitContext {
        branch: git_output(base_dir, &["branch", "--show-current"]),
        status: git_lines(base_dir, &["status", "--short"], 20),
        remotes: git_lines(base_dir, &["remote", "-v"], 10)
            .into_iter()
            .filter_map(|line| simplify_remote(&line))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

fn git_output(base_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(base_dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn git_lines(base_dir: &Path, args: &[&str], max_lines: usize) -> Vec<String> {
    git_output(base_dir, args)
        .map(|value| {
            value
                .lines()
                .take(max_lines)
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn simplify_remote(line: &str) -> Option<String> {
    let (name, rest) = line.split_once(char::is_whitespace)?;
    let url = rest
        .trim()
        .trim_end_matches("(fetch)")
        .trim_end_matches("(push)")
        .trim();
    if url.is_empty() {
        return None;
    }

    let trimmed = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("ssh://");
    let without_user = trimmed.split('@').next_back().unwrap_or(trimmed);
    Some(format!("{name} -> {without_user}"))
}

fn package_scripts(path: PathBuf) -> Option<Vec<String>> {
    let raw = fs::read_to_string(path).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let scripts = json.get("scripts")?.as_object()?;
    let mut keys: Vec<String> = scripts.keys().cloned().collect();
    keys.sort();
    Some(keys)
}

fn go_module_name(path: PathBuf) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    raw.lines()
        .find_map(|line| line.strip_prefix("module ").map(ToString::to_string))
}

fn compose_services(base_dir: &Path) -> Option<Vec<String>> {
    let file = [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ]
    .into_iter()
    .map(|name| base_dir.join(name))
    .find(|path| path.exists())?;
    parse_compose_services(&fs::read_to_string(file).ok()?)
}

fn parse_compose_services(raw: &str) -> Option<Vec<String>> {
    let mut services = BTreeSet::new();
    let mut in_services = false;

    for line in raw.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_services = line.trim() == "services:";
            continue;
        }

        if !in_services {
            continue;
        }

        if let Some(name) = line
            .strip_prefix("  ")
            .and_then(|value| value.strip_suffix(':'))
        {
            let name = name.trim();
            if !name.is_empty() && !name.contains(' ') {
                services.insert(name.to_string());
            }
        }
    }

    if services.is_empty() {
        None
    } else {
        Some(services.into_iter().collect())
    }
}

fn has_kubernetes_manifests(base_dir: &Path) -> bool {
    if base_dir.join("k8s").exists() {
        return true;
    }

    top_level_yaml_files(base_dir).into_iter().any(|path| {
        fs::read_to_string(path)
            .map(|raw| {
                raw.lines()
                    .any(|line| line.trim_start().starts_with("apiVersion:"))
            })
            .unwrap_or(false)
    })
}

fn top_level_yaml_files(base_dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(base_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .map(|ext| matches!(ext, "yml" | "yaml"))
                .unwrap_or(false)
        })
        .collect()
}

fn make_targets(path: PathBuf) -> Option<Vec<String>> {
    let raw = fs::read_to_string(path).ok()?;
    let regex = Regex::new(r"^([A-Za-z0-9][A-Za-z0-9_.-]+):(?:\s|$)").ok()?;
    let mut targets = BTreeSet::new();

    for line in raw.lines() {
        if let Some(captures) = regex.captures(line)
            && let Some(target) = captures.get(1)
        {
            let value = target.as_str();
            if !value.starts_with('.') {
                targets.insert(value.to_string());
            }
        }
    }

    if targets.is_empty() {
        None
    } else {
        Some(targets.into_iter().collect())
    }
}

fn has_tf_files(base_dir: &Path) -> bool {
    fs::read_dir(base_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .map(|ext| ext == "tf")
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::{make_targets, parse_compose_services};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parses_compose_services() {
        let services = parse_compose_services(
            r#"
services:
  api:
    image: app
  worker:
    image: worker
"#,
        )
        .unwrap();
        assert_eq!(services, vec!["api".to_string(), "worker".to_string()]);
    }

    #[test]
    fn parses_make_targets() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Makefile");
        fs::write(
            &path,
            r#"
build:
test:
.PHONY: build test
"#,
        )
        .unwrap();

        let targets = make_targets(path).unwrap();
        assert_eq!(targets, vec!["build".to_string(), "test".to_string()]);
    }
}
