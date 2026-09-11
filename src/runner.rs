use std::{fmt, future::Future, io, time::Instant};

use futures_util::StreamExt;
use tokio::time::{Duration, timeout_at};

use crate::{
    config::Target,
    output::AnswerWriter,
    provider::{Event, EventStream, PromptProvider, ProviderError, Request, Usage},
};

#[derive(Debug)]
pub enum RunError {
    Provider(String),
    Timeout(u64),
    Output(io::Error),
}

impl RunError {
    pub fn is_broken_pipe(&self) -> bool {
        matches!(self, Self::Output(error) if error.kind() == io::ErrorKind::BrokenPipe)
    }

    pub const fn is_output(&self) -> bool {
        matches!(self, Self::Output(_))
    }

    /// A text-free failure category suitable for statistics.
    pub const fn class(&self) -> &'static str {
        match self {
            Self::Provider(_) => "provider",
            Self::Timeout(_) => "timeout",
            Self::Output(_) => "output",
        }
    }

    fn provider(error: ProviderError) -> Self {
        Self::Provider(error.to_string())
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(message) => write!(formatter, "provider request failed: {message}"),
            Self::Timeout(milliseconds) => {
                write!(
                    formatter,
                    "provider request timed out after {milliseconds} ms"
                )
            }
            Self::Output(error) => write!(formatter, "cannot write answer: {error}"),
        }
    }
}

/// Everything one query produced, whether it finished or failed. `answer` is
/// the raw text received from the provider, before stdout normalization.
pub struct Outcome {
    pub answer: String,
    pub usage: Option<Usage>,
    pub api: Duration,
    pub first_token: Option<Duration>,
    pub error: Option<RunError>,
}

pub async fn run<P: PromptProvider, W: io::Write>(
    provider: &P,
    target: &Target,
    request: Request<'_>,
    output: &mut AnswerWriter<'_, W>,
) -> Outcome {
    let mut progress = Progress::new();
    let error = progress.stream(provider, target, request, output).await;
    progress.into_outcome(error.err())
}

struct Progress {
    start: Instant,
    answer: String,
    usage: Option<Usage>,
    first_token: Option<Duration>,
    api: Option<Duration>,
}

impl Progress {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            answer: String::new(),
            usage: None,
            first_token: None,
            api: None,
        }
    }

    async fn stream<P: PromptProvider, W: io::Write>(
        &mut self,
        provider: &P,
        target: &Target,
        request: Request<'_>,
        output: &mut AnswerWriter<'_, W>,
    ) -> Result<(), RunError> {
        let limit = Limit::new(target.timeout_ms);
        let started = limit.within(provider.start(request)).await?;
        let mut stream = started.map_err(RunError::provider)?;
        while let Some(event) = limit
            .next(&mut stream)
            .await
            .map_err(|error| after_partial(output, error))?
        {
            self.accept(event, output)?;
        }
        self.api = Some(self.start.elapsed());
        output.finish(true).map_err(RunError::Output)
    }

    fn accept<W: io::Write>(
        &mut self,
        event: Event,
        output: &mut AnswerWriter<'_, W>,
    ) -> Result<(), RunError> {
        match event {
            Event::Text(text) => {
                let start = self.start;
                self.first_token.get_or_insert_with(|| start.elapsed());
                self.answer.push_str(&text);
                output.write_chunk(&text).map_err(RunError::Output)
            }
            Event::Usage(reported) => {
                self.usage = reported;
                Ok(())
            }
            Event::Other => Ok(()),
        }
    }

    fn into_outcome(self, error: Option<RunError>) -> Outcome {
        Outcome {
            api: self.api.unwrap_or_else(|| self.start.elapsed()),
            answer: self.answer,
            usage: self.usage,
            first_token: self.first_token,
            error,
        }
    }
}

/// The single request deadline shared by connection and every stream item.
struct Limit {
    deadline: tokio::time::Instant,
    timeout_ms: u64,
}

impl Limit {
    fn new(timeout_ms: u64) -> Self {
        Self {
            deadline: tokio::time::Instant::now() + Duration::from_millis(timeout_ms),
            timeout_ms,
        }
    }

    async fn within<F: Future>(&self, future: F) -> Result<F::Output, RunError> {
        timeout_at(self.deadline, future)
            .await
            .map_err(|_| RunError::Timeout(self.timeout_ms))
    }

    async fn next(&self, stream: &mut EventStream) -> Result<Option<Event>, RunError> {
        match self.within(stream.next()).await? {
            None => Ok(None),
            Some(item) => item.map(Some).map_err(RunError::provider),
        }
    }
}

/// Ends a partial answer. The provider or timeout failure that stopped the
/// stream stays the reported error even if finishing stdout also fails.
fn after_partial<W: io::Write>(output: &mut AnswerWriter<'_, W>, original: RunError) -> RunError {
    let _ = output.finish(false);
    original
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_pipe_is_recognized() {
        let error = RunError::Output(io::Error::from(io::ErrorKind::BrokenPipe));
        assert!(error.is_broken_pipe());
        assert!(!RunError::Timeout(1).is_broken_pipe());
    }

    #[test]
    fn errors_have_concise_context() {
        assert_eq!(
            RunError::Provider("bad body".to_string()).to_string(),
            "provider request failed: bad body"
        );
        assert_eq!(
            RunError::Timeout(25).to_string(),
            "provider request timed out after 25 ms"
        );
    }

    /// Accepts answer text, then reports a closed pipe for the final newline.
    struct ClosedAfterText(bool);

    impl io::Write for ClosedAfterText {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            self.0 = true;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_closed_pipe_while_finishing_keeps_the_provider_failure() {
        let mut stdout = ClosedAfterText(false);
        let mut output = AnswerWriter::new(&mut stdout);
        output.write_chunk("partial").unwrap();
        let error = after_partial(&mut output, RunError::Provider("reset".into()));
        assert_eq!(error.class(), "provider");
    }

    #[test]
    fn classes_carry_no_message_text() {
        let output = RunError::Output(io::Error::other("detail"));
        assert_eq!(RunError::Provider("detail".into()).class(), "provider");
        assert_eq!(RunError::Timeout(1).class(), "timeout");
        assert_eq!(output.class(), "output");
        assert!(output.is_output());
        assert!(!RunError::Timeout(1).is_output());
    }
}
