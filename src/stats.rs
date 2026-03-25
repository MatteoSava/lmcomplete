use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
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
        let path = state_dir().join(STATS_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }

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
        let dir = state_dir();
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create stats directory {}", dir.display()))?;
        let path = dir.join(STATS_FILE);
        let raw = serde_json::to_string_pretty(self)?;
        fs::write(&path, raw)
            .with_context(|| format!("failed to write stats file {}", path.display()))?;
        Ok(())
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::UsageStats;

    #[test]
    fn default_stats_are_zeroed() {
        let stats = UsageStats::default();
        assert_eq!(stats.requests, 0);
        assert_eq!(stats.total_cost_credits, 0.0);
    }
}
