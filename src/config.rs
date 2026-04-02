use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub provider: ProviderConfig,
    pub history: HistoryConfig,
    pub streaming: StreamingConfig,
    pub expand: ExpandConfig,
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProviderKind {
    OpenRouter,
    OpenAiCompatible,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub max_entries: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct StreamingConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ExpandConfig {
    pub response_mode: ExpandResponseMode,
    pub explain_display: ExplainDisplay,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExpandResponseMode {
    #[default]
    ToolCall,
    MessageJson,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExplainDisplay {
    #[default]
    Both,
    Inline,
    Message,
    Off,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: default_provider_name(),
            api_key: None,
            model: default_model(),
            base_url: default_base_url(),
            fallback: None,
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

impl Default for StreamingConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Config {
    pub fn load(path_override: Option<&Path>) -> Result<Self> {
        let path = path_override
            .map(Path::to_path_buf)
            .unwrap_or_else(default_config_path);

        if !path.exists() {
            let mut config = Self::default();
            config.normalize();
            config.apply_env_overrides();
            return Ok(config);
        }

        validate_permissions(&path)?;

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        let mut config: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file {}", path.display()))?;
        config.normalize();
        config.apply_env_overrides();
        Ok(config)
    }

    pub fn provider_api_key(&self) -> Option<&str> {
        self.provider.api_key.as_deref()
    }

    pub fn provider_kind(&self) -> Result<ProviderKind> {
        provider_kind(&self.provider.name).ok_or_else(|| {
            anyhow!(
                "unsupported provider '{}'; supported providers: openrouter, openai_compatible, ollama, lm_studio",
                self.provider.name
            )
        })
    }

    pub fn require_provider_config(&self) -> Result<()> {
        let primary_kind = self.provider_kind()?;
        let config_path = default_config_path();

        if self.provider.base_url.trim().is_empty() {
            bail!("provider base_url must not be empty");
        }

        if primary_kind == ProviderKind::OpenAiCompatible
            && self.provider.base_url == default_base_url()
        {
            bail!(
                "provider '{}' requires an explicit base_url in {}",
                self.provider.name,
                config_path.display()
            );
        }

        if primary_kind == ProviderKind::OpenRouter && self.provider_api_key().is_none() {
            bail!(
                "missing provider API key; set OPENROUTER_API_KEY, LMC_PROVIDER_API_KEY, or configure {}",
                config_path.display()
            );
        }

        if let Some(fallback) = &self.provider.fallback {
            provider_kind(&fallback.name).ok_or_else(|| {
                anyhow!(
                    "unsupported fallback provider '{}'; supported providers: openrouter, openai_compatible, ollama, lm_studio",
                    fallback.name
                )
            })?;

            if fallback.name != self.provider.name {
                bail!(
                    "fallback provider '{}' must match primary provider '{}'",
                    fallback.name,
                    self.provider.name
                );
            }
        }

        Ok(())
    }

    fn normalize(&mut self) {
        if self.provider.fallback.is_none() && self.provider.name == "openrouter" {
            self.provider.fallback = Some(FallbackProviderConfig::default());
        }
    }

    fn apply_env_overrides(&mut self) {
        if self.provider.api_key.is_none()
            && let Some(value) = non_empty_env_var("LMC_PROVIDER_API_KEY")
        {
            self.provider.api_key = Some(value);
            return;
        }

        if self.provider.api_key.is_none()
            && self.provider.name == "openrouter"
            && let Some(value) = non_empty_env_var("OPENROUTER_API_KEY")
        {
            self.provider.api_key = Some(value);
        }
    }
}

pub fn default_config_path() -> PathBuf {
    resolve_config_dir(env_path("XDG_CONFIG_HOME"), env_path("HOME")).join("config.toml")
}

pub fn state_dir() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    Ok(dirs
        .state_dir()
        .unwrap_or(dirs.data_local_dir())
        .to_path_buf())
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("", "", "lmcomplete")
        .ok_or_else(|| anyhow!("unable to determine lmcomplete config directories"))
}

fn resolve_config_dir(xdg_config_home: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    if let Some(path) = xdg_config_home {
        return path.join("lmcomplete");
    }

    if let Some(path) = home {
        return path.join(".config").join("lmcomplete");
    }

    project_dirs()
        .expect("lmcomplete should always resolve platform config directories")
        .config_dir()
        .to_path_buf()
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn non_empty_env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn default_provider_name() -> String {
    "openrouter".to_string()
}

fn default_model() -> String {
    "openai/gpt-oss-120b:groq".to_string()
}

fn default_base_url() -> String {
    "https://openrouter.ai/api/v1/chat/completions".to_string()
}

fn provider_kind(name: &str) -> Option<ProviderKind> {
    match name {
        "openrouter" => Some(ProviderKind::OpenRouter),
        "openai_compatible" | "ollama" | "lm_studio" => Some(ProviderKind::OpenAiCompatible),
        _ => None,
    }
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
    use super::{Config, ExpandResponseMode, ExplainDisplay, resolve_config_dir};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn defaults_to_env_api_key_when_config_missing() {
        let _guard = env_lock().lock().unwrap();
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("missing.toml");

        unsafe { std::env::set_var("OPENROUTER_API_KEY", "env-key") };
        let config = Config::load(Some(&config_path)).unwrap();
        unsafe { std::env::remove_var("OPENROUTER_API_KEY") };

        assert_eq!(config.provider_api_key(), Some("env-key"));
        assert_eq!(config.expand.response_mode, ExpandResponseMode::ToolCall);
        assert_eq!(config.expand.explain_display, ExplainDisplay::Both);
    }

    #[test]
    fn prefers_generic_env_api_key_when_present() {
        let _guard = env_lock().lock().unwrap();
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("missing.toml");

        unsafe { std::env::set_var("LMC_PROVIDER_API_KEY", "generic-key") };
        unsafe { std::env::set_var("OPENROUTER_API_KEY", "openrouter-key") };
        let config = Config::load(Some(&config_path)).unwrap();
        unsafe { std::env::remove_var("LMC_PROVIDER_API_KEY") };
        unsafe { std::env::remove_var("OPENROUTER_API_KEY") };

        assert_eq!(config.provider_api_key(), Some("generic-key"));
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

[expand]
response_mode = "message_json"
explain_display = "inline"
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
        assert_eq!(config.expand.response_mode, ExpandResponseMode::MessageJson);
        assert_eq!(config.expand.explain_display, ExplainDisplay::Inline);
    }

    #[test]
    fn documented_config_example_parses() {
        let config: Config = toml::from_str(include_str!("../docs/config.example.toml")).unwrap();

        assert_eq!(config.provider.name, "openrouter");
        assert_eq!(config.provider.model, "openai/gpt-oss-120b:groq");
        assert_eq!(
            config.provider.base_url,
            "https://openrouter.ai/api/v1/chat/completions"
        );

        let fallback = config.provider.fallback.as_ref().unwrap();
        assert_eq!(fallback.name, "openrouter");
        assert_eq!(fallback.model, "anthropic/claude-3.5-sonnet");

        assert_eq!(config.history.max_entries, 20);
        assert_eq!(config.expand.response_mode, ExpandResponseMode::ToolCall);
        assert_eq!(config.expand.explain_display, ExplainDisplay::Both);
        assert!(config.streaming.enabled);
    }

    #[test]
    fn accepts_ollama_without_api_key_when_base_url_is_explicit() {
        let config: Config = toml::from_str(
            r#"
[provider]
name = "ollama"
model = "llama3.2"
base_url = "http://localhost:11434/v1/chat/completions"
"#,
        )
        .unwrap();

        assert!(config.require_provider_config().is_ok());
    }

    #[test]
    fn rejects_openai_compatible_provider_without_explicit_base_url() {
        let config: Config = toml::from_str(
            r#"
[provider]
name = "openai_compatible"
model = "gpt-4.1-mini"
"#,
        )
        .unwrap();

        let error = config.require_provider_config().unwrap_err();
        assert!(error.to_string().contains("requires an explicit base_url"));
    }

    #[test]
    fn prefers_xdg_config_home_for_default_config_dir() {
        let dir = resolve_config_dir(
            Some(PathBuf::from("/tmp/custom-config")),
            Some(PathBuf::from("/Users/tester")),
        );
        assert_eq!(dir, PathBuf::from("/tmp/custom-config/lmcomplete"));
    }

    #[test]
    fn falls_back_to_home_config_dir_when_xdg_config_home_is_missing() {
        let dir = resolve_config_dir(None, Some(PathBuf::from("/Users/tester")));
        assert_eq!(dir, PathBuf::from("/Users/tester/.config/lmcomplete"));
    }
}
