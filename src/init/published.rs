//! The model list the `ask` project publishes, offered by `ask init` when a
//! hosted preset has no key.
//!
//! The request carries no credential or query content, refuses redirects, and
//! is bounded in time and size. The document is untrusted: only version 1 is
//! accepted, and identifiers are sanitized and ordered as a provider's own
//! list is. Any failure falls back to manual entry.

use std::{
    env,
    io::{BufRead, Write},
    time::Duration,
};

use rig_core::serde_json::{self, Value};

use super::{Dialogue, InitError, one_line, provider::hosted_preset};
use crate::{config::ProviderConfig, model_list, provider};

pub(super) const DEFAULT_URL: &str =
    "https://raw.githubusercontent.com/mdml/ask/models/v1/models.json";
/// Overrides [`DEFAULT_URL`]; an empty value disables the published list.
pub(super) const URL_VARIABLE: &str = "ASK_MODEL_LIST_URL";
const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const VERSION: u64 = 1;

/// The identifiers published for one provider, and the date of the list.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Published {
    pub ids: Vec<String>,
    pub generated: String,
}

impl<R: BufRead, W: Write> Dialogue<'_, R, W> {
    /// A model chosen from the published list when `provider` is a hosted
    /// preset, otherwise entered as free text.
    pub(super) async fn published_model(
        &mut self,
        provider: &ProviderConfig,
    ) -> Result<String, InitError> {
        let (Some(name), Some(url)) = (hosted_preset(provider), self.published_url()?) else {
            return self.manual_model();
        };
        self.say(&format!(
            "Requesting the published model list from {}; no credentials are sent.",
            one_line(&url)
        ))?;
        match fetch(&url, name).await {
            Ok(list) => {
                self.say(&format!(
                    "Published model list generated {}; any identifier can still be entered.",
                    list.generated
                ))?;
                self.pick_model(list.ids)
            }
            Err(reason) => {
                self.say(&format!(
                    "Cannot use the published model list ({}); enter the identifier manually.",
                    one_line(&reason)
                ))?;
                self.manual_model()
            }
        }
    }

    /// The list's location, or `None` when the override disables it.
    fn published_url(&mut self) -> Result<Option<String>, InitError> {
        match (self.lookup)(URL_VARIABLE) {
            Ok(url) => Ok((!url.is_empty()).then_some(url)),
            Err(env::VarError::NotPresent) => Ok(Some(DEFAULT_URL.to_string())),
            Err(env::VarError::NotUnicode(_)) => {
                self.say(&format!(
                    "{URL_VARIABLE} is not valid Unicode; skipping the published model list."
                ))?;
                Ok(None)
            }
        }
    }
}

/// Requests the document at `url` and returns its list for `provider`.
pub(super) async fn fetch(url: &str, provider: &str) -> Result<Published, String> {
    parse(&download(url).await?, provider)
}

async fn download(url: &str) -> Result<Vec<u8>, String> {
    let client = provider::http_client().map_err(|error| error.to_string())?;
    let mut response = client
        .get(url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(transport)?;
    let status = response.status();
    if status.is_redirection() {
        return Err(format!("HTTP status {status}; redirects are not followed"));
    }
    if !status.is_success() {
        return Err(format!("HTTP status {status}"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("the response is larger than 1 MiB".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn transport(error: reqwest::Error) -> String {
    if error.is_timeout() {
        return format!("no complete response within {} seconds", TIMEOUT.as_secs());
    }
    error.to_string()
}

/// Accepts only a version 1 document with a `generated_at` time and a
/// nonempty list for `provider`.
pub(super) fn parse(body: &[u8], provider: &str) -> Result<Published, String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| "the response is not valid JSON".to_string())?;
    if value["version"].as_u64() != Some(VERSION) {
        return Err("the list has an unsupported format or version".to_string());
    }
    let generated = generated_date(&value["generated_at"])
        .ok_or_else(|| "the list has no valid generated_at time".to_string())?;
    let listed = value["providers"][provider]
        .as_array()
        .into_iter()
        .flatten();
    let ids = model_list::arrange(
        listed
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
    );
    if ids.is_empty() {
        return Err(format!("the list has no models for {provider}"));
    }
    Ok(Published { ids, generated })
}

/// The `YYYY-MM-DD` date of an RFC 3339 time; the rest is never shown.
fn generated_date(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    let date = text.get(..10)?;
    let shaped = date.bytes().enumerate().all(|(index, byte)| match index {
        4 | 7 => byte == b'-',
        _ => byte.is_ascii_digit(),
    });
    let time_follows = matches!(text.as_bytes().get(10), Some(b'T' | b't'));
    (shaped && time_follows).then(|| date.to_string())
}

#[cfg(test)]
#[path = "published_tests.rs"]
mod tests;
