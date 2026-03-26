use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const OPENROUTER_PROVIDER: &str = "openrouter";
const OLLAMA_PROVIDER: &str = "ollama";

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
            model: default_openrouter_model(),
            base_url: default_openrouter_base_url(),
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
        config.normalize_provider_defaults(provider_field_presence(&raw)?);
        config.apply_env_overrides();
        Ok(config)
    }

    pub fn provider_api_key(&self) -> Option<&str> {
        self.provider.api_key.as_deref()
    }

    pub fn require_provider_config(&self) -> Result<()> {
        match self.provider.name.as_str() {
            OPENROUTER_PROVIDER => {
                if self.provider_api_key().is_none() {
                    bail!(
                        "missing provider API key; set OPENROUTER_API_KEY or configure {}",
                        default_config_path().display()
                    );
                }
                self.require_matching_fallback(OPENROUTER_PROVIDER)
            }
            OLLAMA_PROVIDER => self.require_matching_fallback(OLLAMA_PROVIDER),
            other => bail!(
                "unsupported provider '{}'; supported providers: 'openrouter', 'ollama'",
                other
            ),
        }
    }

    fn apply_env_overrides(&mut self) {
        let env_var = match self.provider.name.as_str() {
            OLLAMA_PROVIDER => "OLLAMA_API_KEY",
            _ => "OPENROUTER_API_KEY",
        };

        if self.provider.api_key.is_none()
            && let Ok(value) = std::env::var(env_var)
            && !value.trim().is_empty()
        {
            self.provider.api_key = Some(value);
        }
    }

    fn normalize_provider_defaults(&mut self, fields: ProviderFieldPresence) {
        if self.provider.name != OLLAMA_PROVIDER {
            return;
        }

        if !fields.model {
            self.provider.model = default_ollama_model();
        }
        if !fields.base_url {
            self.provider.base_url = default_ollama_base_url();
        }
        if !fields.fallback {
            self.provider.fallback = None;
        }
    }

    fn require_matching_fallback(&self, provider_name: &str) -> Result<()> {
        if let Some(fallback) = &self.provider.fallback
            && fallback.name != provider_name
        {
            bail!(
                "unsupported fallback provider '{}' for '{}'; fallback provider must also be '{}'",
                fallback.name,
                provider_name,
                provider_name
            );
        }

        Ok(())
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
    OPENROUTER_PROVIDER.to_string()
}

fn default_openrouter_model() -> String {
    "meta-llama/llama-4-scout".to_string()
}

fn default_openrouter_base_url() -> String {
    "https://openrouter.ai/api/v1/chat/completions".to_string()
}

fn default_ollama_model() -> String {
    "qwen2.5-coder".to_string()
}

fn default_ollama_base_url() -> String {
    "http://127.0.0.1:11434/api/chat".to_string()
}

#[derive(Debug, Clone, Copy, Default)]
struct ProviderFieldPresence {
    model: bool,
    base_url: bool,
    fallback: bool,
}

fn provider_field_presence(raw: &str) -> Result<ProviderFieldPresence> {
    let value: toml::Value =
        toml::from_str(raw).context("failed to inspect config structure for provider defaults")?;
    let Some(table) = value.get("provider").and_then(toml::Value::as_table) else {
        return Ok(ProviderFieldPresence::default());
    };

    Ok(ProviderFieldPresence {
        model: table.contains_key("model"),
        base_url: table.contains_key("base_url"),
        fallback: table.contains_key("fallback"),
    })
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
    use super::{Config, default_ollama_base_url, default_ollama_model};
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

    #[test]
    fn loads_ollama_defaults_when_provider_selected() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
[provider]
name = "ollama"
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        let config = Config::load(Some(&config_path)).unwrap();
        assert_eq!(config.provider.name, "ollama");
        assert_eq!(config.provider.model, default_ollama_model());
        assert_eq!(config.provider.base_url, default_ollama_base_url());
        assert!(config.provider.fallback.is_none());
    }

    #[test]
    fn ollama_provider_can_use_env_api_key() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
[provider]
name = "ollama"
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        unsafe { std::env::set_var("OLLAMA_API_KEY", "ollama-env-key") };
        let config = Config::load(Some(&config_path)).unwrap();
        unsafe { std::env::remove_var("OLLAMA_API_KEY") };

        assert_eq!(config.provider_api_key(), Some("ollama-env-key"));
    }

    #[test]
    fn ollama_provider_does_not_require_an_api_key() {
        let mut config = Config::default();
        config.provider.name = "ollama".to_string();
        config.provider.model = default_ollama_model();
        config.provider.base_url = default_ollama_base_url();
        config.provider.fallback = None;

        config.require_provider_config().unwrap();
    }
}
