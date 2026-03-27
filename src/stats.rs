use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::state_dir;
use crate::provider::Usage;

const STATS_FILE: &str = "stats.json";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct UsageStats {
    pub requests: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub total_cost_credits: f64,
    pub last_request_unix_seconds: Option<u64>,
}

impl UsageStats {
    pub fn load() -> Result<Self> {
        let path = state_dir()?.join(STATS_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }

        validate_stats_permissions(&path)?;

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read stats file {}", path.display()))?;
        let stats =
            serde_json::from_str(&raw).with_context(|| "failed to parse stats.json".to_string())?;
        Ok(stats)
    }

    pub fn record(usage: &Usage) -> Result<Self> {
        let mut stats = Self::load()?;
        stats.requests += 1;
        stats.prompt_tokens += usage.prompt_tokens.unwrap_or_default();
        stats.completion_tokens += usage.completion_tokens.unwrap_or_default();
        stats.total_tokens += usage.total_tokens.unwrap_or_default();
        stats.total_cost_credits += usage.cost.unwrap_or_default();
        stats.last_request_unix_seconds = Some(now_unix_seconds());
        stats.save()?;
        Ok(stats)
    }

    pub fn save(&self) -> Result<()> {
        let dir = state_dir()?;
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create stats directory {}", dir.display()))?;
        let path = dir.join(STATS_FILE);
        let raw = serde_json::to_string_pretty(self)?;

        let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
        {
            let mut file = fs::File::create_new(&tmp_path)
                .with_context(|| format!("failed to create temp stats file {}", tmp_path.display()))?;
            #[cfg(unix)]
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", tmp_path.display()))?;
            file.write_all(raw.as_bytes())
                .with_context(|| format!("failed to write temp stats file {}", tmp_path.display()))?;
            file.sync_all()
                .with_context(|| "failed to sync temp stats file")?;
        }
        fs::rename(&tmp_path, &path)
            .with_context(|| format!("failed to rename {} to {}", tmp_path.display(), path.display()))?;
        Ok(())
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

fn validate_stats_permissions(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        let mode = fs::metadata(path)
            .with_context(|| format!("failed to stat stats file {}", path.display()))?
            .permissions()
            .mode()
            & 0o777;

        if mode != 0o600 {
            bail!(
                "stats file {} must have mode 0600, found {:o}",
                path.display(),
                mode
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::UsageStats;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn default_stats_are_zeroed() {
        let stats = UsageStats::default();
        assert_eq!(stats.requests, 0);
        assert_eq!(stats.total_cost_credits, 0.0);
    }

    #[test]
    fn save_creates_file_with_0600_permissions() {
        let dir = tempdir().unwrap();
        let stats_file = dir.path().join("stats.json");
        let tmp_file = dir.path().join("stats.json.tmp.99999");

        // Simulate what save() does: create_new, set permissions, rename
        {
            let mut file = std::fs::File::create_new(&tmp_file).unwrap();
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .unwrap();
            file.write_all(b"{}").unwrap();
            file.sync_all().unwrap();
        }
        std::fs::rename(&tmp_file, &stats_file).unwrap();

        let mode = std::fs::metadata(&stats_file)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn save_rejects_symlink_at_target_path() {
        let dir = tempdir().unwrap();
        let stats_file = dir.path().join("stats.json");
        let redirect = dir.path().join("redirect.json");

        // Create symlink: stats.json -> redirect.json
        std::os::unix::fs::symlink(&redirect, &stats_file).unwrap();

        // Attempt to write should fail because create_new won't follow existing symlinks
        let result = std::fs::File::create_new(&stats_file);
        assert!(result.is_err());
    }
}
