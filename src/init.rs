//! Interactive creation of a fresh configuration file.
//!
//! The dialogue uses terminal selection menus or a line-oriented fallback for
//! redirected stdin. Every prompt and diagnostic goes to `stderr`; nothing
//! is written to stdout. End of input at any prompt cancels without writing.
//! `init` never replaces an existing configuration; `ask configure apply`
//! is the command that installs a replacement.
//!
//! A provider with no credential variable, such as a local model server,
//! needs no key: init lists its models and verifies the setup with the fixed
//! placeholder queries send.
//!
//! A credential is taken from the environment or, on an attended terminal, a
//! hidden prompt. It is used only to list models and verify the setup during
//! `init`, and is never written or shown.

mod model;
mod provider;

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fmt,
    io::{self, BufRead, Write},
    path::Path,
};

use crate::{
    config::{Config, ProfileConfig, ProviderConfig},
    configure::{self, WriteError},
    validate::Value,
};

const DEFAULT_PROFILE_NAME: &str = "default";

/// Reads one environment variable, as [`env::var`] does.
type Lookup = fn(&str) -> Result<String, env::VarError>;

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
                "configuration already exists at '{path}'; 'ask configure apply' replaces regular files only"
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

/// Where the dialogue reads and writes, and what the terminal supports.
pub struct Console<'a, R, W> {
    pub input: &'a mut R,
    pub output: &'a mut W,
    /// Selection menus are available: stdin and stderr are terminals and
    /// `TERM` is usable.
    pub menus: bool,
    /// Stdin and stderr are terminals, so a key may be read at a hidden prompt.
    pub attended: bool,
}

/// The provider chosen for one target.
struct Selection {
    name: String,
    provider: ProviderConfig,
    /// How a local server is usually started, shown when it cannot be reached.
    start_hint: Option<&'static str>,
}

struct Dialogue<'a, R, W> {
    console: Console<'a, R, W>,
    lookup: Lookup,
}

/// Creates `path` from answers read on `input`. Refuses to touch an existing file.
pub async fn run<R: BufRead, W: Write>(
    path: &Path,
    console: Console<'_, R, W>,
) -> Result<(), InitError> {
    start(path, console, |name| env::var(name)).await
}

async fn start<R: BufRead, W: Write>(
    path: &Path,
    console: Console<'_, R, W>,
    lookup: Lookup,
) -> Result<(), InitError> {
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(InitError::Exists(path.display().to_string()));
    }
    let mut dialogue = Dialogue { console, lookup };
    dialogue.say(&format!("Creating '{}'.", path.display()))?;
    let config = dialogue.collect().await?;
    let rendered = config
        .to_toml()
        .map_err(|error| InitError::Failed(error.to_string()))?;
    dialogue.say("\nConfiguration to write:\n")?;
    dialogue.say(&rendered)?;
    if !dialogue.confirm("Write this configuration? [y/N]: ")? {
        return Err(InitError::Cancelled);
    }
    configure::create_new(path, rendered.as_bytes()).map_err(|error| failed(path, &error))?;
    dialogue.say(&format!("Wrote '{}'.", path.display()))?;
    dialogue.next_steps(&config)
}

fn failed(path: &Path, error: &WriteError) -> InitError {
    match error {
        WriteError::Exists => InitError::Exists(path.display().to_string()),
        WriteError::Failed(_) => InitError::Failed(configure::cannot_write(path, error)),
    }
}

/// The variable a provider's credential is read from, if it needs one.
fn credential_variable(provider: &ProviderConfig) -> Option<&str> {
    provider.api_key_env.as_deref()
}

/// Collapses a diagnostic to one line of printable text.
fn one_line(text: &str) -> String {
    text.split(|character: char| character.is_whitespace() || character.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

impl<R: BufRead, W: Write> Dialogue<'_, R, W> {
    async fn collect(&mut self) -> Result<Config, InitError> {
        self.introduction()?;
        let mut config = Config {
            default_profile: String::new(),
            expire_history: false,
            history_days: None,
            providers: BTreeMap::new(),
            profiles: BTreeMap::new(),
        };
        loop {
            self.add_target(&mut config).await?;
            if !self.confirm("\nAdd another provider? [y/N]: ")? {
                return Ok(config);
            }
        }
    }

    /// Adds one provider and the one profile that uses it.
    async fn add_target(&mut self, config: &mut Config) -> Result<(), InitError> {
        let Selection {
            name: provider_name,
            provider,
            start_hint,
        } = self.provider(&config.providers)?;
        let keyless = credential_variable(&provider).is_none();
        let key = match credential_variable(&provider) {
            Some(variable) => self.credential(variable)?,
            None => None,
        };
        let model = self.model(&provider, key.as_ref(), start_hint).await?;
        let system_prompt = self.optional_prompt()?;
        let profile_name = self.profile_name(config, &provider_name)?;
        if config.default_profile.is_empty() {
            config.default_profile.clone_from(&profile_name);
        }
        let profile = ProfileConfig {
            provider: provider_name.clone(),
            model,
            system_prompt,
            max_output_tokens: None,
        };
        config.providers.insert(provider_name, provider);
        config.profiles.insert(profile_name.clone(), profile);
        if key.is_some() || keyless {
            self.verify(config, &profile_name, key.as_ref()).await
        } else {
            Ok(())
        }
    }

    fn introduction(&mut self) -> Result<(), InitError> {
        self.say("ask stores provider settings in configuration and reads credentials from environment variables only.")?;
        self.say("During init, a key from the environment or a hidden prompt lists models and verifies the setup; it is never written.")?;
        self.say("")
    }

    fn profile_name(&mut self, config: &Config, provider_name: &str) -> Result<String, InitError> {
        let suggestion = if config.profiles.is_empty() {
            DEFAULT_PROFILE_NAME
        } else {
            provider_name
        };
        let prompt = format!("Profile name [{suggestion}]: ");
        loop {
            let name = self
                .optional(&prompt)?
                .unwrap_or_else(|| suggestion.to_string());
            if !config.profiles.contains_key(&name) {
                return Ok(name);
            }
            self.say("That profile name is already used; choose another.")?;
        }
    }

    fn next_steps(&mut self, config: &Config) -> Result<(), InitError> {
        let variables: BTreeSet<&str> = config
            .providers
            .values()
            .filter_map(credential_variable)
            .collect();
        let default = &config.profiles[&config.default_profile].provider;
        self.say("\nNext steps:")?;
        if !variables.is_empty() {
            self.say(&format!(
                "  Supply {} only to the ask process; docs/guides/credentials.md shows how without a shell-wide export.",
                variables.into_iter().collect::<Vec<_>>().join(", ")
            ))?;
        }
        let Some(variable) = credential_variable(&config.providers[default]) else {
            return self.say("  Ask a first question: ask 'what is 2+2'");
        };
        self.say("  For example, this bash/zsh command prompts invisibly for the key and asks a first question:")?;
        self.say(&format!(
            "    ( printf 'API key: ' >&2; IFS= read -rs {variable} </dev/tty || exit; printf '\\n' >&2; export {variable}; exec ask 'what is 2+2' )"
        ))?;
        self.say(
            "  With the key already in the environment, ask a first question: ask 'what is 2+2'",
        )
    }

    fn optional_prompt(&mut self) -> Result<Option<String>, InitError> {
        self.say(&format!(
            "Default system prompt: {}",
            crate::DEFAULT_SYSTEM_PROMPT
        ))?;
        self.optional("Replacement system prompt (empty keeps the default): ")
    }

    /// Offers numbered `labels` and returns the chosen position.
    fn numbered(&mut self, title: &str, labels: &[String]) -> Result<usize, InitError> {
        self.say(&format!("{title}:"))?;
        for (index, label) in labels.iter().enumerate() {
            self.say(&format!(" {}. {label}", index + 1))?;
        }
        let count = labels.len();
        loop {
            let answer = self.ask(&format!("{title} [1-{count}]: "))?;
            match answer.parse::<usize>() {
                Ok(number) if (1..=count).contains(&number) => return Ok(number - 1),
                _ => self.say(&format!("Enter a number from 1 to {count}."))?,
            }
        }
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
        if self.console.menus {
            write!(self.console.output, "? {prompt}")?;
        } else {
            write!(self.console.output, "{prompt}")?;
        }
        self.console.output.flush()?;
        let mut line = String::new();
        if self.console.input.read_line(&mut line)? == 0 {
            writeln!(self.console.output)?;
            return Err(InitError::Cancelled);
        }
        Ok(line.trim().to_string())
    }

    fn say(&mut self, text: &str) -> Result<(), InitError> {
        writeln!(self.console.output, "{text}")?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "init_tests.rs"]
mod tests;
