use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub provider: ProviderConfig,
    pub history: HistoryConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub name: String,
    pub api_key: Option<String>,
    pub model: String,
    pub base_url: String,
    pub fallback: Option<FallbackProviderConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FallbackProviderConfig {
    pub name: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub max_entries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: ProviderConfig::default(),
            history: HistoryConfig::default(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: default_provider_name(),
            api_key: None,
            model: default_model(),
            base_url: default_base_url(),
            fallback: Some(FallbackProviderConfig::default()),
        }
    }
}

impl Default for FallbackProviderConfig {
    fn default() -> Self {
        Self {
            name: default_provider_name(),
            model: "anthropic/claude-3.5-sonnet".to_string(),
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { max_entries: 20 }
    }
}

impl Config {
    pub fn load(path_override: Option<&Path>) -> Result<Self> {
        let path = path_override
            .map(Path::to_path_buf)
            .unwrap_or_else(default_config_path);

        if !path.exists() {
            let mut config = Self::default();
            config.apply_env_overrides();
            return Ok(config);
        }

        validate_permissions(&path)?;

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        let mut config: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file {}", path.display()))?;
        config.apply_env_overrides();
        Ok(config)
    }

    pub fn provider_api_key(&self) -> Option<&str> {
        self.provider.api_key.as_deref()
    }

    pub fn require_provider_config(&self) -> Result<()> {
        if self.provider.name != "openrouter" {
            bail!(
                "unsupported provider '{}'; only 'openrouter' is implemented in v1",
                self.provider.name
            );
        }

        if self.provider_api_key().is_none() {
            bail!(
                "missing provider API key; set OPENROUTER_API_KEY or configure {}",
                default_config_path().display()
            );
        }

        if let Some(fallback) = &self.provider.fallback
            && fallback.name != "openrouter"
        {
            bail!(
                "unsupported fallback provider '{}'; only 'openrouter' is implemented in v1",
                fallback.name
            );
        }

        Ok(())
    }

    fn apply_env_overrides(&mut self) {
        if self.provider.api_key.is_none()
            && let Ok(value) = std::env::var("OPENROUTER_API_KEY")
            && !value.trim().is_empty()
        {
            self.provider.api_key = Some(value);
        }
    }
}

pub fn default_config_path() -> PathBuf {
    project_dirs().config_dir().join("config.toml")
}

pub fn state_dir() -> PathBuf {
    let dirs = project_dirs();
    dirs.state_dir()
        .unwrap_or(dirs.data_local_dir())
        .to_path_buf()
}

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from("", "", "lmcomplete")
        .ok_or_else(|| anyhow!("unable to determine lmcomplete config directories"))
        .expect("lmcomplete should always resolve platform config directories")
}

fn default_provider_name() -> String {
    "openrouter".to_string()
}

fn default_model() -> String {
    "meta-llama/llama-4-scout".to_string()
}

fn default_base_url() -> String {
    "https://openrouter.ai/api/v1/chat/completions".to_string()
}

fn validate_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let mode = fs::metadata(path)
            .with_context(|| format!("failed to stat config file {}", path.display()))?
            .permissions()
            .mode()
            & 0o777;

        if mode != 0o600 {
            bail!(
                "config file {} must have mode 0600, found {:o}",
                path.display(),
                mode
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Config;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn defaults_to_env_api_key_when_config_missing() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("missing.toml");

        unsafe { std::env::set_var("OPENROUTER_API_KEY", "env-key") };
        let config = Config::load(Some(&config_path)).unwrap();
        unsafe { std::env::remove_var("OPENROUTER_API_KEY") };

        assert_eq!(config.provider_api_key(), Some("env-key"));
    }

    #[test]
    fn loads_config_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
[provider]
name = "openrouter"
api_key = "file-key"
model = "model-x"
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        let config = Config::load(Some(&config_path)).unwrap();
        assert_eq!(config.provider_api_key(), Some("file-key"));
        assert_eq!(config.provider.model, "model-x");
    }
}
