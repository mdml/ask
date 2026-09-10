//! The single strict validator for a complete `ask` configuration document.
//!
//! Ordinary configuration loading, `ask configure check`, `ask configure apply`,
//! and any later diagnostic all go through [`document`], so a configuration that
//! `ask` accepts on one path is accepted on every other path.
//!
//! Diagnostics use schema fields and numeric entry indexes, or syntax locations. They never repeat the candidate document's values,
//! and validation never reads a credential or contacts a provider.

use std::fmt;

use crate::config::{Config, ProfileConfig, ProviderConfig};

pub(crate) const PROVIDER_KIND: &str = "openai-compatible";

const ENDPOINT_RULE: &str = "must be an http:// or https:// URL with a host, no embedded credentials, and no query or fragment component";
const ENV_VAR_RULE: &str = "must be an environment variable name: letters, digits, and underscores, not starting with a digit";
const REQUIRED_RULE: &str = "must not be empty";

/// One value taken from a candidate document, checked against a single rule.
#[derive(Clone, Copy)]
pub(crate) struct Value<'a>(pub(crate) &'a str);

/// One dotted configuration key, so a diagnostic points at a single field.
struct Key<'a> {
    table: &'a str,
    entry: usize,
    field: &'a str,
}

/// The text of a candidate document, used to locate a failure within it.
struct Document<'a>(&'a str);

/// Parses and fully validates a candidate configuration document.
pub(crate) fn document(contents: &str) -> Result<Config, String> {
    let value: toml::Value =
        toml::from_str(contents).map_err(|error| Document(contents).describe(&error))?;
    schema::check(&value)?;
    let config: Config = value
        .try_into()
        .map_err(|_| "configuration: invalid schema value".to_string())?;
    check(&config)?;
    Ok(config)
}

fn check(config: &Config) -> Result<(), String> {
    for (index, (name, provider)) in config.providers.iter().enumerate() {
        check_provider(index + 1, name, provider)?;
    }
    for (index, (name, profile)) in config.profiles.iter().enumerate() {
        check_profile(config, index + 1, name, profile)?;
    }
    if config.profiles.contains_key(&config.default_profile) {
        return Ok(());
    }
    Err(missing_profile(&config.default_profile))
}

fn check_provider(index: usize, name: &str, provider: &ProviderConfig) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("providers[{index}].name {REQUIRED_RULE}"));
    }
    if provider.kind != PROVIDER_KIND {
        return Err(format!(
            "providers[{index}].kind has an unsupported kind; the only supported kind is '{PROVIDER_KIND}'"
        ));
    }
    field(
        Key {
            table: "providers",
            entry: index,
            field: "base_url",
        },
        endpoint(Value(&provider.base_url)),
    )?;
    field(
        Key {
            table: "providers",
            entry: index,
            field: "api_key_env",
        },
        env_var_name(Value(&provider.api_key_env)),
    )?;
    field(
        Key {
            table: "providers",
            entry: index,
            field: "timeout_ms",
        },
        positive(provider.timeout_ms),
    )
}

fn check_profile(
    config: &Config,
    index: usize,
    name: &str,
    profile: &ProfileConfig,
) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("profiles[{index}].name {REQUIRED_RULE}"));
    }
    if !config.providers.contains_key(&profile.provider) {
        return Err(format!(
            "profiles[{index}].provider references an unknown provider"
        ));
    }
    field(
        Key {
            table: "profiles",
            entry: index,
            field: "model",
        },
        non_empty(Value(&profile.model)),
    )?;
    Ok(())
}

pub(crate) fn missing_profile(_name: &str) -> String {
    "default_profile references an unknown profile".to_string()
}

pub(crate) fn missing_provider(_name: &str, _profile: &ProfileConfig) -> String {
    "profiles.provider references an unknown provider".to_string()
}

fn field(key: Key<'_>, result: Result<(), &'static str>) -> Result<(), String> {
    result.map_err(|rule| format!("{key} {rule}"))
}

const fn positive(value: u64) -> Result<(), &'static str> {
    if value == 0 {
        return Err("must be greater than zero");
    }
    Ok(())
}

impl fmt::Display for Key<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}[{}].{}", self.table, self.entry, self.field)
    }
}

pub(crate) fn non_empty(value: Value<'_>) -> Result<(), &'static str> {
    if value.0.is_empty() {
        return Err(REQUIRED_RULE);
    }
    Ok(())
}

pub(crate) fn endpoint(value: Value<'_>) -> Result<(), &'static str> {
    let valid = value
        .0
        .parse::<rig_core::http_client::Uri>()
        .ok()
        .is_some_and(valid_endpoint);
    if valid && !value.0.contains('#') {
        return Ok(());
    }
    Err(ENDPOINT_RULE)
}

pub(crate) fn env_var_name(value: Value<'_>) -> Result<(), &'static str> {
    let mut characters = value.0.chars();
    let starts_well = characters.next().is_some_and(starts_env_var_name);
    let continues_well = characters.all(continues_env_var_name);
    if starts_well && continues_well {
        return Ok(());
    }
    Err(ENV_VAR_RULE)
}

fn valid_endpoint(uri: rig_core::http_client::Uri) -> bool {
    let http = matches!(uri.scheme_str(), Some("http" | "https"));
    let host = uri.host().is_some_and(|host| !host.is_empty());
    let credentials = uri
        .authority()
        .is_some_and(|authority| authority.as_str().contains('@'));
    let query = uri.query().is_some();
    http && host && !credentials && !query
}

fn starts_env_var_name(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn continues_env_var_name(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

impl Document<'_> {
    /// Turns a deserialization failure into a one-line diagnostic that carries
    /// the location and the schema problem but none of the document's values.
    fn describe(&self, error: &toml::de::Error) -> String {
        let message = "invalid TOML syntax".to_string();
        match error.span() {
            Some(span) => {
                let (line, column) = self.position(span.start);
                format!("{message} (line {line}, column {column})")
            }
            None => message,
        }
    }

    fn position(&self, offset: usize) -> (usize, usize) {
        let head = self.0.get(..offset).unwrap_or(self.0);
        let line = head.matches('\n').count() + 1;
        let column = head.rsplit('\n').next().unwrap_or_default().chars().count() + 1;
        (line, column)
    }
}

#[path = "schema.rs"]
mod schema;

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
