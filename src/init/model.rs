//! The credential, model, and verification steps for one provider.

use std::{
    env,
    io::{BufRead, Write},
};

use super::{Dialogue, InitError, one_line};
use crate::{
    config::{Config, ProviderConfig},
    model_fetch::Listing,
    provider::{self, Kind},
    validate,
};

const MANUAL_ENTRY: &str = "Enter a model identifier manually";

/// A key held in memory for the duration of `init`. It has no `Display` or
/// `Debug` implementation so it cannot reach a diagnostic by accident.
pub(super) struct Secret(String);

impl<R: BufRead, W: Write> Dialogue<'_, R, W> {
    /// The key from `variable`, else one pasted at a hidden prompt on an
    /// attended terminal, else `None`. Never reads a key from redirected stdin.
    pub(super) fn credential(&mut self, variable: &str) -> Result<Option<Secret>, InitError> {
        match (self.lookup)(variable) {
            Ok(value) if !value.is_empty() => {
                self.say(&format!(
                    "Using {variable} from the environment (value not shown)."
                ))?;
                return Ok(Some(Secret(value)));
            }
            Err(env::VarError::NotUnicode(_)) => {
                self.say(&format!(
                    "{variable} is set but is not valid Unicode; ignoring it."
                ))?;
            }
            _ => {}
        }
        if !self.console.attended {
            self.say(&format!(
                "{variable} is not set; skipping the model list and verification."
            ))?;
            return Ok(None);
        }
        self.hidden_key(variable)
    }

    fn hidden_key(&mut self, variable: &str) -> Result<Option<Secret>, InitError> {
        self.say(&format!(
            "{variable} is not set. Paste the key to list models and verify the setup; it is used only during init and never written."
        ))?;
        let prompt = if self.console.menus {
            "? API key (hidden; Enter skips): "
        } else {
            "API key (hidden; Enter skips): "
        };
        let key = crate::terminal::read_hidden(prompt)?.ok_or(InitError::Cancelled)?;
        let key = key.trim();
        if key.is_empty() {
            self.say("No key entered; skipping the model list and verification.")?;
            return Ok(None);
        }
        Ok(Some(Secret(key.to_string())))
    }

    /// A model identifier chosen from the provider's list when a key is
    /// available, otherwise entered as free text.
    pub(super) async fn model(
        &mut self,
        provider: &ProviderConfig,
        key: Option<&Secret>,
    ) -> Result<String, InitError> {
        let (Some(key), Some(kind)) = (key, Kind::parse(&provider.kind)) else {
            return self.manual_model();
        };
        self.say(&format!(
            "Requesting the model list from {}.",
            provider.base_url
        ))?;
        let listing = Listing {
            kind,
            base_url: &provider.base_url,
            credential: &key.0,
        };
        match listing.fetch().await {
            Ok(ids) if !ids.is_empty() => self.pick_model(ids),
            Ok(_) => {
                self.say("The provider listed no models; enter the identifier manually.")?;
                self.manual_model()
            }
            Err(reason) => {
                self.say(&format!(
                    "Cannot list models ({}); enter the identifier manually.",
                    one_line(&reason)
                ))?;
                self.manual_model()
            }
        }
    }

    fn pick_model(&mut self, ids: Vec<String>) -> Result<String, InitError> {
        let mut labels = ids;
        labels.push(MANUAL_ENTRY.to_string());
        let choice = if self.console.menus {
            crate::terminal::select_filtered(
                "Select a model (type to filter, arrow keys, Enter; Esc cancels):",
                &labels,
                1,
            )?
            .ok_or(InitError::Cancelled)?
        } else {
            self.numbered("Select a model", &labels)?
        };
        labels.pop();
        match labels.into_iter().nth(choice) {
            Some(model) => {
                self.say(&format!("Model: {model}"))?;
                Ok(model)
            }
            None => self.manual_model(),
        }
    }

    fn manual_model(&mut self) -> Result<String, InitError> {
        self.required(
            "Model identifier (free text sent to the provider): ",
            validate::non_empty,
        )
    }

    /// Sends the `ask doctor --live` request to the new profile's target.
    pub(super) async fn verify(
        &mut self,
        config: &Config,
        profile: &str,
        key: &Secret,
    ) -> Result<(), InitError> {
        let target = config
            .resolve_named(profile)
            .map_err(|error| InitError::Failed(error.to_string()))?;
        self.say(&format!("warning: {}", crate::doctor::COST_NOTICE))?;
        let Err(reason) = crate::doctor::live_request(&target, Some(key.0.clone())).await else {
            return self.say("Verified: the provider answered a minimal request.");
        };
        let reason = one_line(&provider::redact(reason, &key.0).to_string());
        self.say(&format!("Verification failed: {reason}"))?;
        if self.confirm("Write the configuration anyway? [y/N]: ")? {
            return Ok(());
        }
        Err(InitError::Cancelled)
    }
}
