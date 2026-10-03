//! Requests a provider's model list for `ask init`.
//!
//! Requests go only to the configured base URL, refuse redirects, carry the
//! credential the way queries do for that kind, and are bounded in time,
//! response size, and page count.

use std::time::Duration;

use crate::{
    model_list::{self, MAX_ENTRIES, Shape},
    provider::{self, GetError, Kind},
};

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PAGES: usize = 10;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// The page size Rig requests when listing Gemini models.
const GEMINI_PAGE_SIZE: &str = "1000";

/// Where one list request goes and the credential header it carries.
#[derive(Debug, PartialEq, Eq)]
pub struct PageRequest {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
}

/// The provider whose list is requested, with the credential for it.
pub struct Listing<'a> {
    pub kind: Kind,
    pub base_url: &'a str,
    pub credential: &'a str,
}

impl Listing<'_> {
    /// Every usable identifier the provider lists, arranged for selection.
    /// Errors never contain the credential.
    pub async fn fetch(&self) -> Result<Vec<String>, String> {
        let client = provider::http_client().map_err(|error| self.redact(error))?;
        let mut ids = Vec::new();
        let mut cursor = None;
        for _ in 0..MAX_PAGES {
            let body = self.body(&client, cursor.as_deref()).await?;
            let page = model_list::parse_page(self.shape(), &body)?;
            ids.extend(page.ids);
            cursor = page.next;
            if cursor.is_none() || ids.len() >= MAX_ENTRIES {
                break;
            }
        }
        Ok(model_list::arrange(ids))
    }

    async fn body(
        &self,
        client: &reqwest::Client,
        cursor: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        let request = self.request(cursor);
        provider::bounded_get(
            client,
            &request.url,
            &request.headers,
            TIMEOUT,
            MAX_RESPONSE_BYTES,
        )
        .await
        .map_err(|error| match error {
            GetError::Transport(error) => self.redact(error),
            GetError::Status(status) => format!("provider returned HTTP status {status}"),
            GetError::TooLarge => "the model list response is too large".to_string(),
        })
    }

    fn redact(&self, error: impl std::fmt::Display) -> String {
        provider::redact(error, self.credential).to_string()
    }

    const fn shape(&self) -> Shape {
        match self.kind {
            Kind::Anthropic => Shape::Anthropic,
            Kind::Gemini => Shape::Gemini,
            Kind::OpenAi | Kind::OpenRouter | Kind::OpenAiCompatible => Shape::OpenAi,
        }
    }

    /// The URL and credential header for one page, following `cursor`.
    pub fn request(&self, cursor: Option<&str>) -> PageRequest {
        let base = self.base_url.trim_end_matches('/');
        let encoded = cursor.map(provider::path_segment);
        match self.kind {
            Kind::Anthropic => PageRequest {
                url: encoded.map_or_else(
                    || format!("{base}/v1/models"),
                    |after| format!("{base}/v1/models?after_id={after}"),
                ),
                headers: vec![
                    ("x-api-key", self.credential.to_string()),
                    ("anthropic-version", provider::ANTHROPIC_VERSION.to_string()),
                ],
            },
            Kind::Gemini => PageRequest {
                url: format!(
                    "{base}/v1beta/models?pageSize={GEMINI_PAGE_SIZE}{}&key={}",
                    encoded.map_or_else(String::new, |token| format!("&pageToken={token}")),
                    provider::path_segment(self.credential)
                ),
                headers: Vec::new(),
            },
            Kind::OpenAi | Kind::OpenRouter | Kind::OpenAiCompatible => PageRequest {
                url: format!("{base}/models"),
                headers: vec![("authorization", format!("Bearer {}", self.credential))],
            },
        }
    }
}

#[cfg(test)]
#[path = "model_fetch_tests.rs"]
mod tests;
