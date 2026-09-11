use std::{future::Future, pin::Pin};

use futures_util::{Stream, StreamExt};
use rig_core::{
    client::CompletionClient,
    completion::{CompletionModel, Message},
    providers::openai,
    streaming::StreamedAssistantContent,
};

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
    Usage(Option<Usage>),
    Other,
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

pub struct RigProvider {
    base_url: String,
    credential: String,
    model: String,
}

impl RigProvider {
    pub fn new(target: &Target, credential: String) -> Self {
        Self {
            base_url: target.base_url.clone(),
            credential,
            model: target.model.clone(),
        }
    }

    fn error(&self, error: impl std::fmt::Display) -> ProviderError {
        redact(error, &self.credential)
    }
}

impl PromptProvider for RigProvider {
    fn start<'a>(&'a self, request: Request<'a>) -> StartFuture<'a> {
        Box::pin(async move {
            let client = openai::Client::builder()
                .api_key(&self.credential)
                .base_url(&self.base_url)
                .build()
                .map_err(|error| self.error(error))?
                .completions_api();
            let model = client.completion_model(&self.model);
            let stream = model
                .completion_request(request.prompt)
                .preamble(request.system_prompt.to_string())
                .messages(history(request.history))
                .stream()
                .await
                .map_err(|error| self.error(error))?;
            let credential = self.credential.clone();
            Ok(Box::pin(
                stream.map(move |item| item.map(event).map_err(|error| redact(error, &credential))),
            ) as EventStream)
        })
    }
}

fn history(exchanges: &[Exchange]) -> Vec<Message> {
    exchanges
        .iter()
        .flat_map(|exchange| {
            [
                Message::user(exchange.prompt.clone()),
                Message::assistant(exchange.answer.clone()),
            ]
        })
        .collect()
}

fn event(content: StreamedAssistantContent) -> Event {
    match content {
        StreamedAssistantContent::Text(text) => Event::Text(text.text),
        StreamedAssistantContent::Final(final_response) => {
            Event::Usage(usage(final_response.usage))
        }
        _ => Event::Other,
    }
}

fn usage(value: rig_core::completion::Usage) -> Option<Usage> {
    let reported = value.input_tokens != 0 || value.output_tokens != 0 || value.total_tokens != 0;
    reported.then_some(Usage {
        input: value.input_tokens,
        output: value.output_tokens,
    })
}

fn redact(error: impl std::fmt::Display, credential: &str) -> ProviderError {
    let message = error.to_string();
    let safe = if credential.is_empty() {
        message
    } else {
        message.replace(credential, "[redacted]")
    };
    ProviderError(safe)
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
    fn zero_usage_means_provider_did_not_report_it() {
        assert!(usage(rig_core::completion::Usage::default()).is_none());
    }
}
