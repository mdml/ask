use std::{
    collections::BTreeMap,
    env, fmt, fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::validate;

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub(crate) const DEFAULT_HISTORY_DAYS: u64 = 90;
const DATABASE_FILE: &str = "ask.sqlite3";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub(crate) default_profile: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) expire_history: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) history_days: Option<u64>,
    pub(crate) providers: BTreeMap<String, ProviderConfig>,
    pub(crate) profiles: BTreeMap<String, ProfileConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub(crate) struct ProfileConfig {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) max_output_tokens: Option<u64>,
}

/// A fully resolved profile. A thread stores this snapshot when it is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub profile: String,
    pub kind: String,
    pub base_url: String,
    pub api_key_env: String,
    pub timeout_ms: u64,
    pub model: String,
    pub system_prompt: String,
    /// The profile's explicit output-token limit, if it set one.
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug)]
pub struct ConfigError(String);

impl Config {
    /// Renders the configuration as TOML in the schema the validator reads.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string(self)
            .map_err(|error| ConfigError(format!("cannot render configuration: {error}")))
    }

    /// The history retention in days, or `None` when history is kept forever.
    pub fn history_days(&self) -> Option<u64> {
        self.expire_history
            .then(|| self.history_days.unwrap_or(DEFAULT_HISTORY_DAYS))
    }

    pub fn resolve(self) -> Result<Target, ConfigError> {
        if !self.profiles.contains_key(&self.default_profile) {
            return Err(ConfigError(validate::missing_profile(
                &self.default_profile,
            )));
        }
        self.resolve_named(&self.default_profile)
    }

    pub fn resolve_named(&self, profile_name: &str) -> Result<Target, ConfigError> {
        let profile = self
            .profiles
            .get(profile_name)
            .ok_or_else(|| ConfigError(format!("profile '{profile_name}' is not configured")))?;
        let provider = self
            .providers
            .get(&profile.provider)
            .ok_or_else(|| ConfigError(validate::missing_provider(profile_name, profile)))?;
        Ok(Target {
            profile: profile_name.to_string(),
            kind: provider.kind.clone(),
            base_url: provider.base_url.clone(),
            api_key_env: provider.api_key_env.clone(),
            timeout_ms: provider.timeout_ms,
            model: profile.model.clone(),
            system_prompt: profile
                .system_prompt
                .clone()
                .unwrap_or_else(|| crate::DEFAULT_SYSTEM_PROMPT.to_string()),
            max_output_tokens: profile.max_output_tokens,
        })
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Reads the installed configuration and validates it with the same strict
/// validator `ask configure check` applies to a candidate document.
pub fn load() -> Result<Config, ConfigError> {
    let path = config_path()?;
    let contents = fs::read_to_string(&path)
        .map_err(|error| ConfigError(format!("cannot read '{}': {error}", path.display())))?;
    validate::document(&contents).map_err(|problem| {
        ConfigError(format!(
            "invalid configuration '{}': {problem}",
            path.display()
        ))
    })
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    located("", ProjectDirs::config_dir, "config.toml", "configuration")
}

/// The history and statistics database: `$ASK_HOME/data/ask.sqlite3`, or
/// `ask.sqlite3` in the platform-standard data directory.
pub fn data_path() -> Result<PathBuf, ConfigError> {
    located("data", ProjectDirs::data_dir, DATABASE_FILE, "data")
}

fn located(
    home_subdirectory: &str,
    platform: fn(&ProjectDirs) -> &Path,
    file: &str,
    kind: &str,
) -> Result<PathBuf, ConfigError> {
    if let Some(home) = env::var_os("ASK_HOME") {
        return Ok(PathBuf::from(home).join(home_subdirectory).join(file));
    }
    ProjectDirs::from("", "", "ask")
        .map(|dirs| platform(&dirs).join(file))
        .ok_or_else(|| ConfigError(format!("platform {kind} directory is unavailable")))
}

const fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

const fn is_false(value: &bool) -> bool {
    !*value
}

const fn is_default_timeout(timeout_ms: &u64) -> bool {
    *timeout_ms == DEFAULT_TIMEOUT_MS
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
