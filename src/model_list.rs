//! Parsing, sanitizing, and ordering a provider's model list for `ask init`.
//!
//! The response is untrusted. Identifiers that could corrupt the terminal or
//! the configuration document are dropped, and the number kept is bounded.
//! Nothing here names a model; the list is whatever the provider reports.

use std::collections::BTreeSet;

use rig_core::serde_json::{self, Value};

use crate::recall::is_invisible_format;

/// The longest identifier kept, in bytes.
pub const MAX_IDENTIFIER_BYTES: usize = 200;
/// The most identifiers kept across every page.
pub const MAX_ENTRIES: usize = 2_000;

/// How a provider's list response is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `{"data": [{"id": ...}]}` in one page.
    OpenAi,
    /// `{"data": [{"id": ...}], "has_more": ..., "last_id": ...}`.
    Anthropic,
    /// `{"models": [{"name": "models/...", ...}], "nextPageToken": ...}`.
    Gemini,
}

/// One parsed page: its identifiers and the cursor for the next page, if any.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub ids: Vec<String>,
    pub next: Option<String>,
}

pub fn parse_page(shape: Shape, body: &[u8]) -> Result<Page, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| "the model list response is not valid JSON".to_string())?;
    Ok(match shape {
        Shape::OpenAi => Page {
            ids: entries(&value["data"], openai_id),
            next: None,
        },
        Shape::Anthropic => Page {
            ids: entries(&value["data"], openai_id),
            next: anthropic_cursor(&value),
        },
        Shape::Gemini => Page {
            ids: entries(&value["models"], gemini_id),
            next: cursor(&value["nextPageToken"]),
        },
    })
}

fn entries(list: &Value, id: fn(&Value) -> Option<&str>) -> Vec<String> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(id)
        .map(str::to_string)
        .collect()
}

/// Drops entries whose listed output modalities exclude text, as
/// OpenRouter reports them; entries without the field are kept.
fn openai_id(entry: &Value) -> Option<&str> {
    let outputs = entry["architecture"]["output_modalities"].as_array();
    if outputs.is_some_and(|modes| !modes.iter().any(|mode| mode == "text")) {
        return None;
    }
    entry["id"].as_str()
}

/// Keeps models that serve `generateContent`, without the `models/` prefix.
fn gemini_id(entry: &Value) -> Option<&str> {
    let methods = entry["supportedGenerationMethods"].as_array()?;
    if !methods.iter().any(|method| method == "generateContent") {
        return None;
    }
    let name = entry["name"].as_str()?;
    Some(name.strip_prefix("models/").unwrap_or(name))
}

fn anthropic_cursor(value: &Value) -> Option<String> {
    if value["has_more"].as_bool() != Some(true) {
        return None;
    }
    cursor(&value["last_id"])
}

fn cursor(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// Whether an identifier is safe to show in a terminal and write to TOML:
/// no control characters, and no invisible format characters that could
/// reorder or hide what a menu shows.
pub fn usable(id: &str) -> bool {
    let hidden = |character: char| character.is_control() || is_invisible_format(character);
    !id.is_empty() && id.len() <= MAX_IDENTIFIER_BYTES && !id.chars().any(hidden)
}

/// Keeps usable, distinct identifiers up to [`MAX_ENTRIES`], in the
/// provider's order except that dated snapshots follow the others.
pub fn arrange(ids: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let kept: Vec<String> = ids
        .into_iter()
        .filter(|id| usable(id) && seen.insert(id.clone()))
        .take(MAX_ENTRIES)
        .collect();
    let (snapshots, others): (Vec<_>, Vec<_>) = kept.into_iter().partition(|id| snapshot(id));
    others.into_iter().chain(snapshots).collect()
}

/// Whether the identifier ends in a date-like suffix: `-YYYYMMDD`,
/// `-YYYY-MM-DD`, `-MMDD`, or `-MM-DD`.
pub fn snapshot(id: &str) -> bool {
    let parts: Vec<&str> = id.rsplit('-').take(2).collect();
    match parts.as_slice() {
        [last, ..] if compact_date(last) => true,
        [day, month] => month_day(month, day),
        _ => false,
    }
}

fn compact_date(text: &str) -> bool {
    match text.len() {
        8 => digits(text, 8) && text.starts_with("20") && month_day(&text[4..6], &text[6..]),
        4 => digits(text, 4) && month_day(&text[..2], &text[2..]),
        _ => false,
    }
}

fn month_day(month: &str, day: &str) -> bool {
    let in_range = |text: &str, max: u8| {
        digits(text, 2)
            && text
                .parse::<u8>()
                .is_ok_and(|value| (1..=max).contains(&value))
    };
    in_range(month, 12) && in_range(day, 31)
}

fn digits(text: &str, count: usize) -> bool {
    text.len() == count && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
#[path = "model_list_tests.rs"]
mod tests;
