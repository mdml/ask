use std::{collections::HashMap, env, fmt, fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub(crate) default_profile: String,
    pub(crate) providers: HashMap<String, ProviderConfig>,
    pub(crate) profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ProviderConfig {
    pub(crate) kind: String,
    pub(crate) base_url: String,
    pub(crate) api_key_env: String,
    #[serde(
        default = "default_timeout_ms",
        skip_serializing_if = "is_default_timeout"
    )]
    pub(crate) timeout_ms: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ProfileConfig {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) system_prompt: Option<String>,
}

#[derive(Debug)]
pub struct Target {
    pub base_url: String,
    pub api_key_env: String,
    pub timeout_ms: u64,
    pub model: String,
    pub system_prompt: String,
}

#[derive(Debug)]
pub struct ConfigError(String);

impl Config {
    /// Renders the configuration as TOML in the schema `load` reads.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string(self)
            .map_err(|error| ConfigError(format!("cannot render configuration: {error}")))
    }

    pub fn resolve(self) -> Result<Target, ConfigError> {
        let profile = self.profiles.get(&self.default_profile).ok_or_else(|| {
            ConfigError(format!(
                "default profile '{}' is not configured",
                self.default_profile
            ))
        })?;
        let provider = self.providers.get(&profile.provider).ok_or_else(|| {
            ConfigError(format!(
                "profile '{}' references unknown provider '{}'",
                self.default_profile, profile.provider
            ))
        })?;
        validate_kind(&profile.provider, &provider.kind)?;
        Ok(Target {
            base_url: provider.base_url.clone(),
            api_key_env: provider.api_key_env.clone(),
            timeout_ms: provider.timeout_ms,
            model: profile.model.clone(),
            system_prompt: profile
                .system_prompt
                .clone()
                .unwrap_or_else(|| crate::DEFAULT_SYSTEM_PROMPT.to_string()),
        })
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub fn load() -> Result<Config, ConfigError> {
    let path = config_path()?;
    let contents = fs::read_to_string(&path)
        .map_err(|error| ConfigError(format!("cannot read '{}': {error}", path.display())))?;
    parse(&contents).map_err(|error| {
        ConfigError(format!(
            "invalid configuration '{}': {error}",
            path.display()
        ))
    })
}

pub(crate) fn parse(contents: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(contents)
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    if let Some(home) = env::var_os("ASK_HOME") {
        return Ok(PathBuf::from(home).join("config.toml"));
    }
    ProjectDirs::from("", "", "ask")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .ok_or_else(|| ConfigError("platform configuration directory is unavailable".to_string()))
}

fn validate_kind(name: &str, kind: &str) -> Result<(), ConfigError> {
    if kind == "openai-compatible" {
        return Ok(());
    }
    Err(ConfigError(format!(
        "provider '{name}' has unsupported kind '{kind}'"
    )))
}

const fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

const fn is_default_timeout(timeout_ms: &u64) -> bool {
    *timeout_ms == DEFAULT_TIMEOUT_MS
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
