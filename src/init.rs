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
const CUSTOM_OPTION: usize = 5;

struct Preset {
    menu_label: &'static str,
    kind: &'static str,
    name: &'static str,
    base_url: &'static str,
    api_key_env: &'static str,
}

const PRESETS: [Preset; 4] = [
    Preset {
        menu_label: "OpenAI",
        kind: "openai",
        name: "openai",
        base_url: "https://api.openai.com/v1",
        api_key_env: "OPENAI_API_KEY",
    },
    Preset {
        menu_label: "Anthropic",
        kind: "anthropic",
        name: "anthropic",
        base_url: "https://api.anthropic.com",
        api_key_env: "ANTHROPIC_API_KEY",
    },
    Preset {
        menu_label: "Gemini",
        kind: "gemini",
        name: "gemini",
        base_url: "https://generativelanguage.googleapis.com",
        api_key_env: "GEMINI_API_KEY",
    },
    Preset {
        menu_label: "OpenRouter",
        kind: "openrouter",
        name: "openrouter",
        base_url: "https://openrouter.ai/api/v1",
        api_key_env: "OPENROUTER_API_KEY",
    },
];

enum ProviderChoice {
    Preset(usize),
    Custom,
}

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
    if std::fs::symlink_metadata(path).is_ok() {
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
        self.introduction()?;
        let choice = self.select_provider()?;
        let (provider_name, provider) = match choice {
            ProviderChoice::Preset(index) => self.preset_provider(&PRESETS[index])?,
            ProviderChoice::Custom => self.custom_provider()?,
        };
        self.credential_instructions(&provider.api_key_env)?;
        let model = self.required(
            "Model identifier (free text sent to the provider): ",
            validate::non_empty,
        )?;
        let system_prompt = self.optional_prompt()?;
        let profile_name = self
            .optional(&format!("Profile name [{DEFAULT_PROFILE_NAME}]: "))?
            .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_string());
        let profile = ProfileConfig {
            provider: provider_name.clone(),
            model,
            system_prompt,
            max_output_tokens: None,
        };
        Ok(Config {
            default_profile: profile_name.clone(),
            expire_history: false,
            history_days: None,
            providers: BTreeMap::from([(provider_name, provider)]),
            profiles: BTreeMap::from([(profile_name, profile)]),
        })
    }

    fn introduction(&mut self) -> Result<(), InitError> {
        self.say(
            "ask stores provider settings in configuration and reads credentials from environment variables only.",
        )?;
        self.say("Export credentials only for the ask process, not in shell profiles or command history.")?;
        self.say(
            "Use a hidden prompt or your existing credential manager to inject the key at launch.",
        )?;
        self.say("See the README and docs/guides/credentials.md for recipes.")?;
        self.say("")
    }

    fn credential_instructions(&mut self, variable: &str) -> Result<(), InitError> {
        self.say("After init succeeds, this bash/zsh command prompts invisibly for your key and asks a first question:")?;
        self.say(&format!(
            "  ( printf 'API key: ' >&2; IFS= read -rs {variable} </dev/tty || exit; printf '\\n' >&2; export {variable}; exec ask 'what is 2+2' )"
        ))?;
        self.say("Paste the key only at that hidden prompt; it stays out of shell history and your parent shell.")
    }

    fn select_provider(&mut self) -> Result<ProviderChoice, InitError> {
        self.say("Select a provider:")?;
        for (index, preset) in PRESETS.iter().enumerate() {
            self.say(&format!(" {}. {}", index + 1, preset.menu_label))?;
        }
        self.say(&format!(
            " {CUSTOM_OPTION}. Custom OpenAI-compatible endpoint"
        ))?;
        loop {
            let answer = self.ask("Select a provider [1-5]: ")?;
            match answer.parse::<usize>() {
                Ok(number) if (1..=PRESETS.len()).contains(&number) => {
                    return Ok(ProviderChoice::Preset(number - 1));
                }
                Ok(number) if number == CUSTOM_OPTION => return Ok(ProviderChoice::Custom),
                _ => self.say("Enter a number from 1 to 5.")?,
            }
        }
    }

    fn preset_provider(&mut self, preset: &Preset) -> Result<(String, ProviderConfig), InitError> {
        self.say(&format!(
            "\nProvider: {} (kind {})",
            preset.menu_label, preset.kind
        ))?;
        self.say(&format!("Endpoint: {}", preset.base_url))?;
        self.say(&format!(
            "Credential variable: {} (value is never read during init)",
            preset.api_key_env
        ))?;
        self.say(&format!(
            "Before your first query, export {} only for the ask process using the credential recipe in the README.",
            preset.api_key_env
        ))?;
        Ok((
            preset.name.to_string(),
            ProviderConfig {
                kind: preset.kind.to_string(),
                base_url: preset.base_url.to_string(),
                api_key_env: preset.api_key_env.to_string(),
                timeout_ms: crate::config::DEFAULT_TIMEOUT_MS,
            },
        ))
    }

    fn custom_provider(&mut self) -> Result<(String, ProviderConfig), InitError> {
        self.say("\nCustom OpenAI-compatible endpoint (kind openai-compatible).")?;
        let provider_name = self.required(
            "Provider name (used in configuration tables): ",
            validate::non_empty,
        )?;
        let base_url = self.required(
            "Endpoint base URL (http:// or https://): ",
            validate::endpoint,
        )?;
        let api_key_env = self.required(
            "Credential environment variable name (value is never read): ",
            validate::env_var_name,
        )?;
        Ok((
            provider_name,
            ProviderConfig {
                kind: validate::PROVIDER_KIND.to_string(),
                base_url,
                api_key_env,
                timeout_ms: crate::config::DEFAULT_TIMEOUT_MS,
            },
        ))
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
