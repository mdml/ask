use std::{future::Future, pin::Pin};

use futures_util::{Stream, StreamExt, future, stream};
use rig_core::{
    Model,
    completion::{CompletionRequest, CompletionResponse, FinishReason, Message},
    driver::Transport,
    error::ProviderError as RigError,
    http_client::DynHttpClient,
    operation::Completion,
    providers::{
        anthropic::{self, AnthropicConfig},
        gemini::GeminiConfig,
        openai::{self, OpenAIConfig},
    },
    serde_json::{Value, json},
    streaming::{Item, StreamEvent, Streamed},
    wire::Wire,
};
use rig_reqwest::ReqwestClient;

use crate::config::Target;

pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event, ProviderError>> + Send>>;
pub type StartFuture<'a> =
    Pin<Box<dyn Future<Output = Result<EventStream, ProviderError>> + Send + 'a>>;

/// A provider-neutral request: the system prompt, prior exchanges in order,
/// then the new user prompt.
pub struct Request<'a> {
    pub prompt: &'a str,
    pub system_prompt: &'a str,
    pub history: &'a [Exchange],
}

/// One prior complete turn supplied as context: the user prompt and the raw
/// answer text as the provider returned it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub prompt: String,
    pub answer: String,
}

pub enum Event {
    Text(String),
    /// The latest token counts reported independently of a terminal record.
    Usage(Usage),
    /// The provider's terminal record: reported usage and how the answer ended.
    Final(Option<Usage>, Ending),
    Other,
}

/// How a provider reported the end of a streamed answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// A natural stop, or any ending `ask` does not treat specially.
    Complete,
    /// The provider stopped the answer at its output-token limit.
    OutputLimit,
    /// The provider declined to answer because of content filtering.
    Refusal,
}

#[derive(Clone, Copy)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

#[derive(Debug)]
pub struct ProviderError(String);

impl std::fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub trait PromptProvider {
    fn start<'a>(&'a self, request: Request<'a>) -> StartFuture<'a>;
}

/// A supported provider `kind`, each served by its own Rig provider module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// OpenAI's Responses API over HTTP server-sent events.
    OpenAi,
    /// Anthropic's Messages API.
    Anthropic,
    /// Gemini's GenerateContent API.
    Gemini,
    /// OpenRouter's Chat Completions API.
    OpenRouter,
    /// Any endpoint implementing OpenAI Chat Completions.
    OpenAiCompatible,
}

/// Every supported `kind` spelling, in the order diagnostics list them.
pub const KINDS: [(&str, Kind); 5] = [
    ("openai", Kind::OpenAi),
    ("anthropic", Kind::Anthropic),
    ("gemini", Kind::Gemini),
    ("openrouter", Kind::OpenRouter),
    ("openai-compatible", Kind::OpenAiCompatible),
];

/// The output-token limit sent when a profile sets none and the provider
/// requires one.
pub const REQUIRED_MAX_OUTPUT_TOKENS: u64 = 4_096;

/// The `anthropic-version` header value Rig sends with Anthropic requests.
pub const ANTHROPIC_VERSION: &str = anthropic::completion::ANTHROPIC_VERSION_LATEST;

impl Kind {
    pub fn parse(name: &str) -> Option<Self> {
        KINDS
            .iter()
            .find(|(spelling, _)| *spelling == name)
            .map(|(_, kind)| *kind)
    }

    /// The profile's limit, or the required default where the provider
    /// rejects a request without one; otherwise the provider's own default.
    pub const fn max_output_tokens(self, configured: Option<u64>) -> Option<u64> {
        match (configured, self) {
            (Some(limit), _) => Some(limit),
            (None, Self::Anthropic) => Some(REQUIRED_MAX_OUTPUT_TOKENS),
            (None, _) => None,
        }
    }
}

/// The bearer value sent for a target with no credential. Rig always sends an
/// `Authorization` header, so a keyless target sends this fixed, non-secret
/// placeholder, which servers that need no key ignore.
pub const NO_KEY_PLACEHOLDER: &str = "no-key";

pub struct RigProvider {
    kind: Option<Kind>,
    base_url: String,
    credential: Option<String>,
    model: String,
    max_output_tokens: Option<u64>,
}

impl RigProvider {
    pub fn new(target: &Target, credential: Option<String>) -> Self {
        Self {
            kind: Kind::parse(&target.kind),
            base_url: target.base_url.clone(),
            credential,
            model: target.model.clone(),
            max_output_tokens: target.max_output_tokens,
        }
    }

    /// The text to redact from diagnostics: empty, so nothing, without a credential.
    fn secret(&self) -> &str {
        self.credential.as_deref().unwrap_or_default()
    }

    fn error(&self, error: impl std::fmt::Display) -> ProviderError {
        redact(error, self.secret())
    }

    fn open(&self, kind: Kind, request: Request<'_>) -> Result<EventStream, ProviderError> {
        let http = http_client().map_err(|error| self.error(error))?;
        let http = DynHttpClient::new(ReqwestClient::from(http));
        let key = self.credential.as_deref().unwrap_or(NO_KEY_PLACEHOLDER);
        let url = self.base_url.as_str();
        let options = |params| StreamOptions {
            max_output_tokens: kind.max_output_tokens(self.max_output_tokens),
            params,
            credential: self.secret(),
        };
        match kind {
            Kind::OpenAi => {
                let client = OpenAIConfig::new(key).with_base_url(url).connect(http);
                // Responses are not retained by OpenAI; `ask` keeps history locally.
                let params = json!({ "store": false });
                stream(
                    client.responses(&self.model),
                    request,
                    options(Some(params)),
                )
            }
            Kind::Anthropic => {
                let client = AnthropicConfig::new(key).with_base_url(url).connect(http);
                stream(client.completion(&self.model), request, options(None))
            }
            Kind::Gemini => {
                // Rig inserts the key and model into the URL verbatim.
                let config = GeminiConfig::new(path_segment(key)).with_base_url(url);
                let client = config.connect(http);
                stream(
                    client.completion(path_segment(&self.model)),
                    request,
                    options(None),
                )
            }
            Kind::OpenRouter => {
                let config = OpenAIConfig::with_key(&openai::wire::OPENROUTER, key);
                let client = config.with_base_url(url).connect(http);
                stream(client.chat(&self.model), request, options(None))
            }
            Kind::OpenAiCompatible => {
                let client = OpenAIConfig::new(key).with_base_url(url).connect(http);
                stream(client.chat(&self.model), request, options(None))
            }
        }
    }
}

impl PromptProvider for RigProvider {
    fn start<'a>(&'a self, request: Request<'a>) -> StartFuture<'a> {
        Box::pin(async move {
            let kind = self
                .kind
                .ok_or_else(|| ProviderError("unsupported provider kind".to_string()))?;
            if kind == Kind::Gemini {
                return self.gemini(request).await;
            }
            self.open(kind, request)
        })
    }
}

impl RigProvider {
    /// Uses Gemini's wire directly because Rig 0.45 turns a prompt-feedback
    /// block in a response with no candidates into a failed stream, not a
    /// refusal ending that carries the response's usage.
    async fn gemini(&self, request: Request<'_>) -> Result<EventStream, ProviderError> {
        let client = http_client().map_err(|error| self.error(error))?;
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url.trim_end_matches('/'),
            path_segment(&self.model),
            path_segment(self.secret())
        );
        let response = client
            .post(url)
            .json(&gemini_request(request, self.max_output_tokens))
            .send()
            .await
            .map_err(|error| self.error(error))?;
        if !response.status().is_success() {
            return Err(self.error(format!(
                "provider returned HTTP status {}",
                response.status()
            )));
        }
        let credential = self.secret().to_string();
        let events = response
            .bytes_stream()
            .map(Some)
            .chain(stream::once(future::ready(None)))
            .scan(Vec::new(), move |buffer, item| {
                let events = match item {
                    Some(Ok(bytes)) => {
                        buffer.extend_from_slice(&bytes);
                        gemini_events(buffer)
                            .into_iter()
                            .map(|event| event.map_err(|error| redact(error, &credential)))
                            .collect()
                    }
                    Some(Err(error)) => vec![Err(redact(error, &credential))],
                    None => gemini_eof(buffer),
                };
                future::ready(Some(events))
            })
            .flat_map(stream::iter);
        Ok(Box::pin(events))
    }
}

fn gemini_request(request: Request<'_>, max_output_tokens: Option<u64>) -> Value {
    let mut contents: Vec<Value> = request
        .history
        .iter()
        .flat_map(|exchange| {
            [
                json!({"role": "user", "parts": [{"text": exchange.prompt}]}),
                json!({"role": "model", "parts": [{"text": exchange.answer}]}),
            ]
        })
        .collect();
    contents.push(json!({"role": "user", "parts": [{"text": request.prompt}]}));
    let mut body = json!({
        "contents": contents,
        "systemInstruction": {"parts": [{"text": request.system_prompt}]},
    });
    if let Some(limit) = max_output_tokens {
        body["generationConfig"] = json!({"maxOutputTokens": limit});
    }
    body
}

/// Removes complete SSE records from `buffer` and maps Gemini response chunks.
fn gemini_events(buffer: &mut Vec<u8>) -> Vec<Result<Event, ProviderError>> {
    gemini_records(buffer)
        .into_iter()
        .flat_map(|data| gemini_event(&data))
        .collect()
}

fn gemini_records(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    while let Some((end, delimiter)) = sse_record_end(buffer) {
        let record: Vec<u8> = buffer.drain(..end + delimiter).collect();
        let mut data = Vec::new();
        let mut saw_data = false;
        for line in record[..end].split(|byte| *byte == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let value = if line == b"data" {
                &[][..]
            } else if let Some(value) = line.strip_prefix(b"data:") {
                value.strip_prefix(b" ").unwrap_or(value)
            } else {
                continue;
            };
            if saw_data {
                data.push(b'\n');
            }
            data.extend_from_slice(value);
            saw_data = true;
        }
        if saw_data {
            records.push(data);
        }
    }
    records
}

fn gemini_eof(buffer: &mut Vec<u8>) -> Vec<Result<Event, ProviderError>> {
    if buffer.iter().all(u8::is_ascii_whitespace) {
        buffer.clear();
        Vec::new()
    } else {
        buffer.clear();
        vec![Err(ProviderError(
            "Gemini stream ended with an incomplete SSE record".to_string(),
        ))]
    }
}

fn sse_record_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer.windows(2).position(|bytes| bytes == b"\n\n");
    let crlf = buffer.windows(4).position(|bytes| bytes == b"\r\n\r\n");
    match (lf, crlf) {
        (Some(left), Some(right)) if left <= right => Some((left, 2)),
        (Some(_), Some(right)) => Some((right, 4)),
        (Some(end), None) => Some((end, 2)),
        (None, Some(end)) => Some((end, 4)),
        (None, None) => None,
    }
}

fn gemini_event(data: &[u8]) -> Vec<Result<Event, ProviderError>> {
    let value: Value = match rig_core::serde_json::from_slice(data) {
        Ok(value) => value,
        Err(error) => {
            return vec![Err(ProviderError(format!(
                "invalid Gemini stream event: {error}"
            )))];
        }
    };
    let reported = gemini_usage(&value["usageMetadata"]);
    let Some(candidate) = gemini_first_candidate(&value) else {
        return gemini_prompt_block_events(&value, reported);
    };
    gemini_candidate_events(candidate, reported)
}

fn gemini_first_candidate(value: &Value) -> Option<&Value> {
    value["candidates"]
        .as_array()
        .and_then(|items| items.first())
}

fn gemini_prompt_block_events(
    value: &Value,
    reported: Option<Usage>,
) -> Vec<Result<Event, ProviderError>> {
    let Some(reason) = value["promptFeedback"]["blockReason"].as_str() else {
        return reported.map(Event::Usage).map(Ok).into_iter().collect();
    };
    if gemini_refusal_reason(reason) {
        vec![Ok(Event::Final(reported, Ending::Refusal))]
    } else {
        vec![Err(ProviderError(format!(
            "unrecognized Gemini prompt block reason: {reason}"
        )))]
    }
}

fn gemini_candidate_events(
    candidate: &Value,
    reported: Option<Usage>,
) -> Vec<Result<Event, ProviderError>> {
    let mut events = gemini_candidate_text_events(candidate);
    if let Some(reported) = reported {
        events.push(Ok(Event::Usage(reported)));
    }
    if let Some(reason) = candidate["finishReason"].as_str() {
        match gemini_map_finish_reason(reason) {
            GeminiFinishMapping::Complete(ending) => {
                events.push(Ok(Event::Final(None, ending)));
            }
            GeminiFinishMapping::Failure(error) => events.push(Err(error)),
        }
    }
    events
}

fn gemini_candidate_text_events(candidate: &Value) -> Vec<Result<Event, ProviderError>> {
    candidate["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| part["thought"].as_bool() != Some(true))
        .filter_map(|part| part["text"].as_str())
        .filter(|text| !text.is_empty())
        .map(|text| Ok(Event::Text(text.to_string())))
        .collect()
}

enum GeminiFinishMapping {
    Complete(Ending),
    Failure(ProviderError),
}

fn gemini_map_finish_reason(reason: &str) -> GeminiFinishMapping {
    match reason {
        "STOP" => GeminiFinishMapping::Complete(Ending::Complete),
        "MAX_TOKENS" => GeminiFinishMapping::Complete(Ending::OutputLimit),
        reason if gemini_refusal_reason(reason) => GeminiFinishMapping::Complete(Ending::Refusal),
        "FINISH_REASON_UNSPECIFIED"
        | "LANGUAGE"
        | "OTHER"
        | "MALFORMED_FUNCTION_CALL"
        | "UNEXPECTED_TOOL_CALL"
        | "TOO_MANY_TOOL_CALLS"
        | "MISSING_THOUGHT_SIGNATURE"
        | "NO_IMAGE"
        | "IMAGE_OTHER"
        | "URL_CONTEXT" => GeminiFinishMapping::Failure(ProviderError(format!(
            "Gemini ended the answer with finish reason: {reason}"
        ))),
        _ => GeminiFinishMapping::Failure(ProviderError(format!(
            "unrecognized Gemini finish reason: {reason}"
        ))),
    }
}

fn gemini_refusal_reason(reason: &str) -> bool {
    matches!(
        reason,
        "SAFETY"
            | "RECITATION"
            | "BLOCKLIST"
            | "PROHIBITED_CONTENT"
            | "SPII"
            | "IMAGE_SAFETY"
            | "IMAGE_PROHIBITED_CONTENT"
            | "MODEL_ARMOR"
    )
}

fn gemini_usage(value: &Value) -> Option<Usage> {
    let input = value["promptTokenCount"].as_u64().unwrap_or_default();
    let output = value["candidatesTokenCount"].as_u64().unwrap_or_default();
    let total = value["totalTokenCount"].as_u64().unwrap_or_default();
    (input != 0 || output != 0 || total != 0).then_some(Usage { input, output })
}

/// The per-call settings that accompany a streamed request.
struct StreamOptions<'a> {
    max_output_tokens: Option<u64>,
    params: Option<Value>,
    /// The text to redact from diagnostics.
    credential: &'a str,
}

/// Opens the streamed completion on `model` and maps it to events: each text
/// fragment, then one terminal record with the reported usage and ending.
fn stream<W>(
    model: Model<W>,
    request: Request<'_>,
    options: StreamOptions<'_>,
) -> Result<EventStream, ProviderError>
where
    W: Wire<Op = Completion>,
    DynHttpClient: Transport<W>,
{
    let StreamOptions {
        max_output_tokens,
        params,
        credential,
    } = options;
    let mut request = CompletionRequest::new(Message::user(request.prompt))
        .preamble(request.system_prompt.to_string())
        .messages(history(request.history))
        .max_tokens(max_output_tokens);
    if let Some(params) = params {
        request = request.additional_params(params);
    }
    let opened = model
        .stream(request)
        .map_err(|error| redact(error, credential))?;
    let state = (Some(opened), credential.to_string());
    Ok(Box::pin(stream::unfold(state, next_event)))
}

type StreamState = (Option<Streamed<Completion>>, String);

async fn next_event(state: StreamState) -> Option<(Result<Event, ProviderError>, StreamState)> {
    let (opened, credential) = state;
    let mut opened = opened?;
    match opened.next().await {
        Some(Ok(Item::Event(StreamEvent::Text { text, .. }))) => {
            Some((Ok(Event::Text(text)), (Some(opened), credential)))
        }
        Some(Ok(_)) => Some((Ok(Event::Other), (Some(opened), credential))),
        // A stream that ends without a terminal record has none to report.
        Some(Err(RigError::Truncated)) => None,
        Some(Err(error)) => Some((Err(redact(error, &credential)), (None, credential))),
        None => {
            let ended = opened.finish().await;
            match ended {
                // A stream that ends without a terminal record has none to report.
                Err(RigError::Truncated) => None,
                ended => {
                    let event = ended
                        .map_err(|error| redact(error, &credential))
                        .and_then(terminal);
                    Some((event, (None, credential)))
                }
            }
        }
    }
}

/// Refuses redirects so the credential and query content reach only the
/// configured endpoint; a redirect response surfaces as a provider failure.
pub(crate) fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

/// Why a [`bounded_get`] returned no body; each caller words it.
pub(crate) enum GetError {
    Transport(reqwest::Error),
    Status(reqwest::StatusCode),
    TooLarge,
}

/// A bounded `GET url` with `headers`, within `timeout` per request and at
/// most `max_bytes` long.
pub(crate) struct BoundedGet<'a> {
    pub url: &'a str,
    pub headers: &'a [(&'a str, String)],
    pub timeout: std::time::Duration,
    pub max_bytes: usize,
}

/// The body of a successful [`BoundedGet`].
pub(crate) async fn bounded_get(
    client: &reqwest::Client,
    request: &BoundedGet<'_>,
) -> Result<Vec<u8>, GetError> {
    let max_bytes = request.max_bytes;
    let mut builder = client.get(request.url).timeout(request.timeout);
    for (name, value) in request.headers {
        builder = builder.header(*name, value);
    }
    let mut response = builder.send().await.map_err(GetError::Transport)?;
    if !response.status().is_success() {
        return Err(GetError::Status(response.status()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(GetError::Transport)? {
        if body.len() + chunk.len() > max_bytes {
            return Err(GetError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Percent-encodes every byte outside RFC 3986's unreserved set, so a value
/// stays one URL path segment or query value whatever characters it holds.
pub(crate) fn path_segment(value: &str) -> String {
    value.bytes().fold(String::new(), |mut encoded, byte| {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
        encoded
    })
}

/// The assistant text sent in place of an empty recorded answer. Rig 0.45
/// drops an empty assistant turn and merges the adjacent user turns when it
/// shapes history for the wire (`completion/history.rs::adapt`), which would
/// fuse two separate questions into one. A fixed marker keeps each question a
/// distinct user turn; the recorded answer stays empty.
pub const NO_ANSWER_MARKER: &str = "[no answer]";

fn history(exchanges: &[Exchange]) -> Vec<Message> {
    exchanges
        .iter()
        .flat_map(|exchange| {
            let answer = if exchange.answer.trim().is_empty() {
                NO_ANSWER_MARKER.to_string()
            } else {
                exchange.answer.clone()
            };
            [
                Message::user(exchange.prompt.clone()),
                Message::assistant(answer),
            ]
        })
        .collect()
}

/// Maps the folded response to the terminal record. An `error` finish reason
/// or a provider-reported failure is an in-band provider failure, such as
/// OpenRouter's mid-stream error chunk, never a complete answer.
fn terminal(response: CompletionResponse) -> Result<Event, ProviderError> {
    let ending = match response.finish_reason() {
        Some(FinishReason::Length) => Ending::OutputLimit,
        Some(FinishReason::ContentFilter) => Ending::Refusal,
        Some(FinishReason::Other(reason)) if reason == "error" => {
            return Err(stream_error());
        }
        _ => Ending::Complete,
    };
    if ending == Ending::Complete && response.error.is_some() {
        return Err(stream_error());
    }
    Ok(Event::Final(usage(&response.usage), ending))
}

fn stream_error() -> ProviderError {
    ProviderError("the provider reported an error in the answer stream".to_string())
}

fn usage(value: &rig_core::completion::Usage) -> Option<Usage> {
    let input = value.input_tokens.unwrap_or(0);
    let output = value.output_tokens.unwrap_or(0);
    let reported = input != 0 || output != 0 || value.total_tokens.unwrap_or(0) != 0;
    reported.then_some(Usage { input, output })
}

/// Replaces every occurrence of the credential, whether it appears verbatim or
/// percent-encoded (in any mix of encoded and literal bytes, either hex case,
/// or with `+` for a space), as it can inside a URL quoted by an HTTP error.
pub(crate) fn redact(error: impl std::fmt::Display, credential: &str) -> ProviderError {
    let message = error.to_string();
    if credential.is_empty() {
        return ProviderError(message);
    }
    let bytes = message.as_bytes();
    let mut safe = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match encoded_match(&bytes[index..], credential) {
            Some(length) => {
                safe.extend_from_slice(b"[redacted]");
                index += length;
            }
            None => {
                safe.push(bytes[index]);
                index += 1;
            }
        }
    }
    ProviderError(String::from_utf8_lossy(&safe).into_owned())
}

/// The length of the prefix of `text` that decodes to `secret`, if any.
fn encoded_match(text: &[u8], secret: &str) -> Option<usize> {
    let mut consumed = 0;
    for expected in secret.chars() {
        let rest = &text[consumed..];
        if let Some(length) = json_escape_match(rest, expected) {
            consumed += length;
            continue;
        }
        let mut utf8 = [0; 4];
        let encoded = expected.encode_utf8(&mut utf8).as_bytes();
        consumed += encoded_bytes_match(rest, encoded, expected == ' ')?;
    }
    Some(consumed)
}

fn encoded_bytes_match(text: &[u8], expected: &[u8], space: bool) -> Option<usize> {
    let mut consumed = 0;
    for (index, &byte) in expected.iter().enumerate() {
        let rest = &text[consumed..];
        let escaped = rest
            .strip_prefix(b"%")
            .and_then(|hex| hex.get(..2))
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        consumed += match (rest.first(), escaped) {
            (_, Some(decoded)) if decoded == byte => 3,
            (Some(&actual), _) if actual == byte => 1,
            (Some(b'+'), _) if space && index == 0 => 1,
            _ => return None,
        };
    }
    Some(consumed)
}

/// Matches a JSON character escape, including a UTF-16 surrogate pair.
fn json_escape_match(text: &[u8], expected: char) -> Option<usize> {
    let simple = match expected {
        '"' => b'"',
        '\\' => b'\\',
        '/' => b'/',
        '\u{0008}' => b'b',
        '\u{000C}' => b'f',
        '\n' => b'n',
        '\r' => b'r',
        '\t' => b't',
        _ => 0,
    };
    if simple != 0 && text.starts_with(&[b'\\', simple]) {
        return Some(2);
    }
    let first = json_u16(text)?;
    if !(0xD800..=0xDFFF).contains(&first) {
        return (char::from_u32(u32::from(first)) == Some(expected)).then_some(6);
    }
    if !(0xD800..=0xDBFF).contains(&first) {
        return None;
    }
    let low = json_u16(text.get(6..)?)?;
    if !(0xDC00..=0xDFFF).contains(&low) {
        return None;
    }
    let scalar = 0x1_0000 + ((u32::from(first) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
    (char::from_u32(scalar) == Some(expected)).then_some(12)
}

fn json_u16(text: &[u8]) -> Option<u16> {
    let hex = text.strip_prefix(b"\\u")?.get(..4)?;
    u16::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_is_redacted_from_errors() {
        let error = redact("request exposed secret-value", "secret-value");
        assert_eq!(error.to_string(), "request exposed [redacted]");
    }

    #[test]
    fn encoded_credentials_are_redacted_from_errors() {
        let credential = "k'ey /+%é";
        for shown in [
            "k'ey /+%é",
            "k%27ey%20%2F%2B%25%C3%A9",
            "k%27ey%20/+%25%c3%a9",
            "k'ey+/%2b%25é",
        ] {
            let error = redact(format!("url (http://x/?key={shown}&alt=sse)"), credential);
            assert_eq!(
                error.to_string(),
                "url (http://x/?key=[redacted]&alt=sse)",
                "{shown}"
            );
        }
        for shown in [r"k\u0027ey \/+%\u00e9", r"k'ey\u0020\u002f+%\u00E9"] {
            let error = redact(format!(r#"{{"error":"{shown}"}}"#), credential);
            assert_eq!(error.to_string(), r#"{"error":"[redacted]"}"#, "{shown}");
        }
        let near_miss = redact("key=k%27ey%20%2F%2B%25%C3", credential);
        assert_eq!(near_miss.to_string(), "key=k%27ey%20%2F%2B%25%C3");
    }

    #[test]
    fn path_segments_encode_everything_but_unreserved_bytes() {
        assert_eq!(path_segment("gemini-2.5_flash~x"), "gemini-2.5_flash~x");
        assert_eq!(
            path_segment("tuned/../m?x=1#f :é"),
            "tuned%2F..%2Fm%3Fx%3D1%23f%20%3A%C3%A9"
        );
    }

    #[test]
    fn only_anthropic_requires_a_default_output_limit() {
        for (_, kind) in KINDS {
            assert_eq!(kind.max_output_tokens(Some(7)), Some(7));
            let expected = (kind == Kind::Anthropic).then_some(REQUIRED_MAX_OUTPUT_TOKENS);
            assert_eq!(kind.max_output_tokens(None), expected);
        }
        assert_eq!(Kind::parse("gemini"), Some(Kind::Gemini));
        assert_eq!(Kind::parse("Gemini"), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn an_unsupported_snapshot_kind_fails_before_any_request() {
        let target = Target {
            profile: "default".to_string(),
            kind: "retired-kind".to_string(),
            base_url: "http://127.0.0.1:1/v1".to_string(),
            api_key_env: Some("KEY".to_string()),
            timeout_ms: 1,
            model: "m".to_string(),
            system_prompt: String::new(),
            max_output_tokens: None,
        };
        let provider = RigProvider::new(&target, Some("secret".to_string()));
        let request = Request {
            prompt: "q",
            system_prompt: "",
            history: &[],
        };
        let error = provider.start(request).await.err().unwrap();
        assert_eq!(error.to_string(), "unsupported provider kind");
    }

    #[test]
    fn the_keyless_placeholder_is_not_redacted() {
        let provider = RigProvider::new(&keyless_target(), None);
        let error = provider.error(format!("sent Bearer {NO_KEY_PLACEHOLDER}"));
        assert_eq!(error.to_string(), "sent Bearer no-key");
    }

    fn keyless_target() -> Target {
        Target {
            profile: "default".to_string(),
            kind: "openai-compatible".to_string(),
            base_url: "http://127.0.0.1:1/v1".to_string(),
            api_key_env: None,
            timeout_ms: 1,
            model: "m".to_string(),
            system_prompt: String::new(),
            max_output_tokens: None,
        }
    }

    #[test]
    fn empty_credentials_do_not_replace_every_boundary() {
        let error = redact("ordinary error", "");
        assert_eq!(error.to_string(), "ordinary error");
    }

    #[test]
    fn history_alternates_user_and_assistant_messages() {
        let exchange = Exchange {
            prompt: "question".to_string(),
            answer: String::new(),
        };
        let messages = history(&[exchange.clone(), exchange]);
        assert_eq!(messages.len(), 4);
        assert!(matches!(messages[0], Message::User { .. }));
        assert!(matches!(messages[1], Message::Assistant { .. }));
        assert!(matches!(messages[2], Message::User { .. }));
    }

    #[test]
    fn empty_answers_are_replayed_as_the_marker() {
        let exchange = |answer: &str| Exchange {
            prompt: "question".to_string(),
            answer: answer.to_string(),
        };
        let messages = history(&[exchange(""), exchange("  \n"), exchange("kept")]);
        assert_eq!(messages[1], Message::assistant(NO_ANSWER_MARKER));
        assert_eq!(messages[3], Message::assistant(NO_ANSWER_MARKER));
        assert_eq!(messages[5], Message::assistant("kept"));
    }

    #[test]
    fn zero_usage_means_provider_did_not_report_it() {
        assert!(usage(&rig_core::completion::Usage::default()).is_none());
    }

    #[test]
    fn gemini_multiline_data_fields_keep_the_sse_newline() {
        let mut buffer = b"data:\ndata: second\n\n".to_vec();
        assert_eq!(gemini_records(&mut buffer), vec![b"\nsecond".to_vec()]);
        assert!(buffer.is_empty());
    }
}
