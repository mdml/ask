//! Changing an existing configuration: one action per run, previewed and
//! confirmed, then installed over the file that was read.
//!
//! The document is re-rendered from the parsed configuration, so every value
//! the action does not touch is kept, but comments and formatting are not.

use std::{
    io::{BufRead, Write},
    path::Path,
};

use super::{Dialogue, InitError, rendered};
use crate::{
    config::{Config, ProviderConfig},
    configure,
    recall::printable,
    validate,
};

const ACTIONS: [&str; 4] = [
    "Add a provider",
    "Add a profile on an existing provider",
    "Change a profile's model",
    "Set the default profile",
];

const NEW_THREADS_ONLY: &str = "The new model applies to new threads only; existing threads keep the profile they were created with.";

const REFORMAT_WARNING: &str = "warning: comments and formatting in the existing file will not be preserved; 'ask configure apply' keeps them.";

const UNCHANGED: &str =
    "nothing was changed; correct the file and validate it with 'ask configure check'";

impl<R: BufRead, W: Write> Dialogue<'_, R, W> {
    /// Applies one chosen change to the valid configuration at `path`.
    pub(super) async fn edit(&mut self, path: &Path) -> Result<(), InitError> {
        let installed = configure::read_installed(path).map_err(unusable)?;
        let mut config = validate::document(&installed.text).map_err(|problem| {
            unusable(format!(
                "invalid configuration '{}': {problem}",
                path.display()
            ))
        })?;
        let reformats = config.to_toml().ok().as_deref() != Some(installed.text.as_str());
        self.say(&format!("Changing '{}'.", path.display()))?;
        self.summary(&config)?;
        self.act(&mut config).await?;
        let rendered = rendered(&config)?;
        self.preview(&rendered)?;
        if reformats {
            self.say(REFORMAT_WARNING)?;
        }
        if !self.confirm("Write this configuration? [y/N]: ")? {
            return Err(InitError::Cancelled);
        }
        configure::replace_installed(path, rendered.as_bytes(), installed)
            .map_err(InitError::Failed)?;
        self.say(&format!("Wrote '{}'.", path.display()))
    }

    fn summary(&mut self, config: &Config) -> Result<(), InitError> {
        self.say(&format!(
            "Default profile: {}",
            printable(&config.default_profile)
        ))?;
        self.say("Profiles:")?;
        for name in config.profiles.keys() {
            self.say(&format!("  {}", profile_label(config, name)))?;
        }
        self.say("")
    }

    async fn act(&mut self, config: &mut Config) -> Result<(), InitError> {
        let labels = ACTIONS.map(String::from);
        match self.choose("Choose a change", &labels)? {
            0 => self.add_target(config).await,
            1 => self.add_profile_on_existing(config).await,
            2 => self.change_model(config).await,
            _ => self.set_default(config),
        }
    }

    async fn add_profile_on_existing(&mut self, config: &mut Config) -> Result<(), InitError> {
        let names: Vec<String> = config.providers.keys().cloned().collect();
        let labels: Vec<String> = config
            .providers
            .iter()
            .map(|(name, provider)| provider_label(name, provider))
            .collect();
        let choice = self.choose("Select a provider", &labels)?;
        self.add_profile(config, &names[choice], None).await
    }

    async fn change_model(&mut self, config: &mut Config) -> Result<(), InitError> {
        let name = self.pick_profile(config, "Select a profile")?;
        let provider = &config.providers[&config.profiles[&name].provider];
        let key = self.key_for(provider)?;
        let model = self.model(provider, key.as_ref(), None).await?;
        self.say(NEW_THREADS_ONLY)?;
        if let Some(profile) = config.profiles.get_mut(&name) {
            profile.model = model;
        }
        self.check_target(config, &name, key.as_ref()).await
    }

    fn set_default(&mut self, config: &mut Config) -> Result<(), InitError> {
        config.default_profile = self.pick_profile(config, "Select the default profile")?;
        Ok(())
    }

    fn pick_profile(&mut self, config: &Config, title: &str) -> Result<String, InitError> {
        let names: Vec<&String> = config.profiles.keys().collect();
        let labels: Vec<String> = names
            .iter()
            .map(|name| profile_label(config, name))
            .collect();
        let choice = self.choose(title, &labels)?;
        Ok(names[choice].clone())
    }
}

fn unusable(problem: String) -> InitError {
    InitError::Failed(format!("{problem}; {UNCHANGED}"))
}

fn profile_label(config: &Config, name: &str) -> String {
    let profile = &config.profiles[name];
    let marker = if name == config.default_profile {
        " (default)"
    } else {
        ""
    };
    format!(
        "{}{marker}: provider {}, model {}",
        printable(name),
        printable(&profile.provider),
        printable(&profile.model)
    )
}

fn provider_label(name: &str, provider: &ProviderConfig) -> String {
    format!(
        "{} ({}, {})",
        printable(name),
        printable(&provider.kind),
        printable(&provider.base_url)
    )
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
