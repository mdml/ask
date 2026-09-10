//! Interactive creation of a fresh configuration file.
//!
//! The dialogue is line oriented so it works with a terminal or redirected
//! stdin. Every prompt and diagnostic goes to the supplied `stderr`; nothing
//! is written to stdout. End of input at any prompt cancels without writing.
//! `init` never replaces an existing configuration; `ask configure apply`
//! is the command that installs a replacement.

use std::{
    collections::BTreeMap,
    fmt,
    io::{self, BufRead, Write},
    path::Path,
};

use crate::{
    config::{Config, ProfileConfig, ProviderConfig},
    configure::{self, WriteError},
    validate::{self, Value},
};

const DEFAULT_PROFILE_NAME: &str = "default";

#[derive(Debug)]
pub enum InitError {
    Cancelled,
    Exists(String),
    Failed(String),
}

impl fmt::Display for InitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("configuration cancelled; nothing was written"),
            Self::Exists(path) => write!(
                formatter,
                "configuration already exists at '{path}'; use 'ask configure apply' to replace it"
            ),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl From<io::Error> for InitError {
    fn from(error: io::Error) -> Self {
        Self::Failed(format!("cannot continue configuration: {error}"))
    }
}

struct Dialogue<'a, R, W> {
    input: &'a mut R,
    output: &'a mut W,
}

/// Creates `path` from answers read on `input`. Refuses to touch an existing file.
pub fn run<R: BufRead, W: Write>(
    path: &Path,
    input: &mut R,
    output: &mut W,
) -> Result<(), InitError> {
    if path.exists() {
        return Err(InitError::Exists(path.display().to_string()));
    }
    let mut dialogue = Dialogue { input, output };
    dialogue.say(&format!("Creating '{}'.", path.display()))?;
    let config = dialogue.collect()?;
    let rendered = config
        .to_toml()
        .map_err(|error| InitError::Failed(error.to_string()))?;
    dialogue.say("\nConfiguration to write:\n")?;
    dialogue.say(&rendered)?;
    if !dialogue.confirm("Write this configuration? [y/N]: ")? {
        return Err(InitError::Cancelled);
    }
    configure::create_new(path, rendered.as_bytes()).map_err(|error| failed(path, &error))?;
    dialogue.say(&format!("Wrote '{}'.", path.display()))
}

fn failed(path: &Path, error: &WriteError) -> InitError {
    match error {
        WriteError::Exists => InitError::Exists(path.display().to_string()),
        WriteError::Failed(_) => InitError::Failed(configure::cannot_write(path, error)),
    }
}

impl<R: BufRead, W: Write> Dialogue<'_, R, W> {
    fn collect(&mut self) -> Result<Config, InitError> {
        let provider_name = self.required("Provider name: ", validate::non_empty)?;
        let base_url = self.required(
            "Endpoint base URL (http:// or https://): ",
            validate::endpoint,
        )?;
        let model = self.required("Model identifier: ", validate::non_empty)?;
        let api_key_env = self.required(
            "Credential environment variable name (value is never read): ",
            validate::env_var_name,
        )?;
        let system_prompt = self.optional_prompt()?;
        let profile_name = self
            .optional(&format!("Profile name [{DEFAULT_PROFILE_NAME}]: "))?
            .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_string());
        let provider = ProviderConfig {
            kind: validate::PROVIDER_KIND.to_string(),
            base_url,
            api_key_env,
            timeout_ms: crate::config::DEFAULT_TIMEOUT_MS,
        };
        let profile = ProfileConfig {
            provider: provider_name.clone(),
            model,
            system_prompt,
        };
        Ok(Config {
            default_profile: profile_name.clone(),
            providers: BTreeMap::from([(provider_name, provider)]),
            profiles: BTreeMap::from([(profile_name, profile)]),
        })
    }

    fn optional_prompt(&mut self) -> Result<Option<String>, InitError> {
        self.say(&format!(
            "Default system prompt: {}",
            crate::DEFAULT_SYSTEM_PROMPT
        ))?;
        self.optional("Replacement system prompt (empty keeps the default): ")
    }

    fn required(
        &mut self,
        prompt: &str,
        rule: fn(Value<'_>) -> Result<(), &'static str>,
    ) -> Result<String, InitError> {
        loop {
            let answer = self.ask(prompt)?;
            match rule(Value(&answer)) {
                Ok(()) => return Ok(answer),
                Err(rule) => self.say(&format!("That value {rule}."))?,
            }
        }
    }

    fn optional(&mut self, prompt: &str) -> Result<Option<String>, InitError> {
        let answer = self.ask(prompt)?;
        Ok((!answer.is_empty()).then_some(answer))
    }

    fn confirm(&mut self, prompt: &str) -> Result<bool, InitError> {
        let answer = self.ask(prompt)?;
        Ok(matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
    }

    fn ask(&mut self, prompt: &str) -> Result<String, InitError> {
        write!(self.output, "{prompt}")?;
        self.output.flush()?;
        let mut line = String::new();
        if self.input.read_line(&mut line)? == 0 {
            writeln!(self.output)?;
            return Err(InitError::Cancelled);
        }
        Ok(line.trim().to_string())
    }

    fn say(&mut self, text: &str) -> Result<(), InitError> {
        writeln!(self.output, "{text}")?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "init_tests.rs"]
mod tests;
