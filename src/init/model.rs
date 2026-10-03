//! The credential, model, and verification steps for one provider.

use std::{
    env,
    io::{BufRead, Write},
};

use super::{Dialogue, InitError, credential_variable, one_line};
use crate::{
    config::{Config, ProviderConfig},
    model_fetch::Listing,
    provider::{self, Kind},
    validate,
};

/// The one line shown before manual entry when a keyed provider has no list.
fn keyed_notice(reason: Option<&str>) -> String {
    match reason {
        Some(reason) => format!("Cannot list models ({reason}); enter the identifier manually."),
        None => "The provider listed no models; enter the identifier manually.".to_string(),
    }
}

/// The one line shown before manual entry when a keyless server has no list.
pub(super) fn keyless_notice(
    base_url: &str,
    reason: Option<&str>,
    start_hint: Option<&str>,
) -> String {
    let reason = reason.unwrap_or("the server listed no models");
    let hint = start_hint.map_or_else(String::new, |hint| format!(" {hint}."));
    format!("Cannot list models from {base_url} ({reason}).{hint} Enter the identifier manually.")
}

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
                "{variable} is not set; continuing without a key, so the setup will not be verified."
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
            self.say(
                "No key entered; continuing without a key, so the setup will not be verified.",
            )?;
            return Ok(None);
        }
        Ok(Some(Secret(key.to_string())))
    }

    /// A model identifier chosen from the provider's list when a key is
    /// available or the provider needs none, otherwise from the published
    /// list for a hosted preset, otherwise entered as free text.
    pub(super) async fn model(
        &mut self,
        provider: &ProviderConfig,
        key: Option<&Secret>,
        start_hint: Option<&str>,
    ) -> Result<String, InitError> {
        let keyless = credential_variable(provider).is_none();
        let credential = match key {
            Some(key) => Some(key.0.as_str()),
            None if keyless => None,
            None => return self.published_model(provider).await,
        };
        let Some(kind) = Kind::parse(&provider.kind) else {
            return self.manual_model();
        };
        self.say(&format!(
            "Requesting the model list from {}.",
            provider.base_url
        ))?;
        let listing = Listing {
            kind,
            base_url: &provider.base_url,
            credential,
        };
        let reason = match listing.fetch().await {
            Ok(ids) if !ids.is_empty() => return self.pick_model(ids),
            Ok(_) => None,
            Err(reason) => Some(one_line(&reason)),
        };
        let notice = if keyless {
            keyless_notice(&provider.base_url, reason.as_deref(), start_hint)
        } else {
            keyed_notice(reason.as_deref())
        };
        self.say(&notice)?;
        self.manual_model()
    }

    pub(super) fn pick_model(&mut self, ids: Vec<String>) -> Result<String, InitError> {
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

    pub(super) fn manual_model(&mut self) -> Result<String, InitError> {
        self.required(
            "Model identifier (free text sent to the provider): ",
            validate::non_empty,
        )
    }

    /// Sends the `ask doctor --live` request to the new profile's target.
    /// Without a `key` the target is keyless, so no cost notice is shown,
    /// although the endpoint may be a remote server.
    pub(super) async fn verify(
        &mut self,
        config: &Config,
        profile: &str,
        key: Option<&Secret>,
    ) -> Result<(), InitError> {
        let target = config
            .resolve_named(profile)
            .map_err(|error| InitError::Failed(error.to_string()))?;
        match key {
            Some(_) => self.say(&format!("warning: {}", crate::doctor::COST_NOTICE))?,
            None => self.say(&format!(
                "Sending a minimal request to {} to verify it.",
                target.base_url
            ))?,
        }
        let secret = key.map(|key| key.0.clone());
        let Err(reason) = crate::doctor::live_request(&target, secret.clone()).await else {
            return self.say("Verified: the provider answered a minimal request.");
        };
        let reason =
            one_line(&provider::redact(reason, secret.as_deref().unwrap_or_default()).to_string());
        self.say(&format!("Verification failed: {reason}"))?;
        if self.confirm("Write the configuration anyway? [y/N]: ")? {
            return Ok(());
        }
        Err(InitError::Cancelled)
    }
}
