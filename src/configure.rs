//! Interactive creation of a fresh configuration file.
//!
//! The dialogue is line oriented so it works with a terminal or redirected
//! stdin. Every prompt and diagnostic goes to the supplied `stderr`; nothing
//! is written to stdout. End of input at any prompt cancels without writing.

use std::{
    collections::HashMap,
    fmt, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::config::{Config, ProfileConfig, ProviderConfig};

const PROVIDER_KIND: &str = "openai-compatible";
const DEFAULT_PROFILE_NAME: &str = "default";

#[derive(Debug)]
pub enum ConfigureError {
    Cancelled,
    Exists(String),
    Failed(String),
}

impl fmt::Display for ConfigureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("configuration cancelled; nothing was written"),
            Self::Exists(path) => write!(
                formatter,
                "configuration already exists at '{path}'; editing it is not yet supported"
            ),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl From<io::Error> for ConfigureError {
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
) -> Result<(), ConfigureError> {
    if path.exists() {
        return Err(ConfigureError::Exists(path.display().to_string()));
    }
    let mut dialogue = Dialogue { input, output };
    dialogue.say(&format!("Creating '{}'.", path.display()))?;
    let config = dialogue.collect()?;
    let rendered = config
        .to_toml()
        .map_err(|error| ConfigureError::Failed(error.to_string()))?;
    dialogue.say("\nConfiguration to write:\n")?;
    dialogue.say(&rendered)?;
    if !dialogue.confirm("Write this configuration? [y/N]: ")? {
        return Err(ConfigureError::Cancelled);
    }
    write_new(path, &rendered)?;
    dialogue.say(&format!("Wrote '{}'.", path.display()))
}

impl<R: BufRead, W: Write> Dialogue<'_, R, W> {
    fn collect(&mut self) -> Result<Config, ConfigureError> {
        let provider_name = self.required("Provider name: ", non_empty)?;
        let base_url = self.required("Endpoint base URL (http:// or https://): ", http_url)?;
        let model = self.required("Model identifier: ", non_empty)?;
        let api_key_env = self.required(
            "Credential environment variable name (value is never read): ",
            env_var_name,
        )?;
        let system_prompt = self.optional_prompt()?;
        let profile_name = self
            .optional(&format!("Profile name [{DEFAULT_PROFILE_NAME}]: "))?
            .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_string());
        let provider = ProviderConfig {
            kind: PROVIDER_KIND.to_string(),
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
            providers: HashMap::from([(provider_name, provider)]),
            profiles: HashMap::from([(profile_name, profile)]),
        })
    }

    fn optional_prompt(&mut self) -> Result<Option<String>, ConfigureError> {
        self.say(&format!(
            "Default system prompt: {}",
            crate::DEFAULT_SYSTEM_PROMPT
        ))?;
        self.optional("Replacement system prompt (empty keeps the default): ")
    }

    fn required(
        &mut self,
        prompt: &str,
        validate: fn(&str) -> Result<(), &'static str>,
    ) -> Result<String, ConfigureError> {
        loop {
            let answer = self.ask(prompt)?;
            match validate(&answer) {
                Ok(()) => return Ok(answer),
                Err(problem) => self.say(problem)?,
            }
        }
    }

    fn optional(&mut self, prompt: &str) -> Result<Option<String>, ConfigureError> {
        let answer = self.ask(prompt)?;
        Ok((!answer.is_empty()).then_some(answer))
    }

    fn confirm(&mut self, prompt: &str) -> Result<bool, ConfigureError> {
        let answer = self.ask(prompt)?;
        Ok(matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
    }

    fn ask(&mut self, prompt: &str) -> Result<String, ConfigureError> {
        write!(self.output, "{prompt}")?;
        self.output.flush()?;
        let mut line = String::new();
        if self.input.read_line(&mut line)? == 0 {
            writeln!(self.output)?;
            return Err(ConfigureError::Cancelled);
        }
        Ok(line.trim().to_string())
    }

    fn say(&mut self, text: &str) -> Result<(), ConfigureError> {
        writeln!(self.output, "{text}")?;
        Ok(())
    }
}

fn non_empty(answer: &str) -> Result<(), &'static str> {
    if answer.is_empty() {
        return Err("A value is required.");
    }
    Ok(())
}

fn http_url(answer: &str) -> Result<(), &'static str> {
    let valid = answer
        .parse::<rig_core::http_client::Uri>()
        .ok()
        .is_some_and(valid_endpoint);
    if valid {
        return Ok(());
    }
    Err("Enter an http:// or https:// URL with a host and no embedded credentials.")
}

fn valid_endpoint(uri: rig_core::http_client::Uri) -> bool {
    let http = matches!(uri.scheme_str(), Some("http" | "https"));
    let host = uri.host().is_some_and(|host| !host.is_empty());
    let credentials = uri
        .authority()
        .is_some_and(|authority| authority.as_str().contains('@'));
    http && host && !credentials
}

fn env_var_name(answer: &str) -> Result<(), &'static str> {
    let mut characters = answer.chars();
    let starts_well = characters.next().is_some_and(starts_env_var_name);
    let continues_well = characters.all(continues_env_var_name);
    let valid = starts_well && continues_well;
    if valid {
        return Ok(());
    }
    Err(
        "Enter an environment variable name: letters, digits, and underscores, not starting with a digit.",
    )
}

fn starts_env_var_name(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn continues_env_var_name(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

/// Publishes a complete file without replacing a destination that appears
/// during the dialogue. A failed write only affects the temporary file.
fn write_new(path: &Path, contents: &str) -> Result<(), ConfigureError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| cannot_write(path, &error))?;
    }
    let mut temporary =
        TemporaryConfig::create(path).map_err(|error| cannot_write(path, &error))?;
    temporary
        .file
        .write_all(contents.as_bytes())
        .and_then(|()| temporary.file.sync_all())
        .map_err(|error| cannot_write(path, &error))?;
    fs::hard_link(&temporary.path, path).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => ConfigureError::Exists(path.display().to_string()),
        _ => cannot_write(path, &error),
    })
}

struct TemporaryConfig {
    path: PathBuf,
    file: fs::File,
}

impl TemporaryConfig {
    fn create(destination: &Path) -> io::Result<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                destination.with_file_name(format!(".ask-config-{}-{id}.tmp", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for TemporaryConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn cannot_write(path: &Path, error: &io::Error) -> ConfigureError {
    ConfigureError::Failed(format!("cannot write '{}': {error}", path.display()))
}

#[cfg(test)]
#[path = "configure_tests.rs"]
mod tests;
