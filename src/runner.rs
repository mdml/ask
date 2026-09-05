use std::{fmt, future::Future, io, time::Instant};

use futures_util::StreamExt;
use tokio::time::{Duration, timeout_at};

use crate::{
    config::Target,
    output::AnswerWriter,
    provider::{Event, PromptProvider, Request},
    stats::Statistics,
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

pub async fn run<P: PromptProvider, W: io::Write>(
    provider: &P,
    target: &Target,
    prompt: &str,
    wall_start: Instant,
    output: &mut AnswerWriter<'_, W>,
) -> Result<Statistics, RunError> {
    let api_start = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(target.timeout_ms);
    let request = Request {
        prompt,
        system_prompt: &target.system_prompt,
    };
    let started = within(deadline, target.timeout_ms, provider.start(request)).await?;
    let mut stream = started.map_err(|error| RunError::Provider(error.to_string()))?;
    let mut first_token = None;
    let mut usage = None;
    loop {
        let item = match within(deadline, target.timeout_ms, stream.next()).await {
            Ok(item) => item,
            Err(error) => return Err(after_partial(output, error)),
        };
        let Some(item) = item else { break };
        let event = match item {
            Ok(event) => event,
            Err(error) => {
                let error = RunError::Provider(error.to_string());
                return Err(after_partial(output, error));
            }
        };
        match event {
            Event::Text(text) => {
                first_token.get_or_insert_with(|| api_start.elapsed());
                output.write_chunk(&text).map_err(RunError::Output)?;
            }
            Event::Usage(reported) => usage = reported,
            Event::Other => {}
        }
    }
    let api = api_start.elapsed();
    output.finish(true).map_err(RunError::Output)?;
    Ok(Statistics {
        model: target.model.clone(),
        wall: wall_start.elapsed(),
        api,
        ttft: first_token.unwrap_or(api),
        usage,
    })
}

async fn within<F: Future>(
    deadline: tokio::time::Instant,
    timeout_ms: u64,
    future: F,
) -> Result<F::Output, RunError> {
    timeout_at(deadline, future)
        .await
        .map_err(|_| RunError::Timeout(timeout_ms))
}

fn after_partial<W: io::Write>(output: &mut AnswerWriter<'_, W>, original: RunError) -> RunError {
    match output.finish(false) {
        Ok(()) => original,
        Err(error) => RunError::Output(error),
    }
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
}
