//! `ask new` and `ask reply`: resolve the profile, stream the answer, then
//! record the query in one transaction.

use std::{
    io,
    process::ExitCode,
    time::{Duration, Instant, SystemTime},
};

use crate::{
    cli::Mode,
    config::{self, Target},
    credential, input,
    output::AnswerWriter,
    provider::{Exchange, Request, RigProvider},
    report,
    runner::{self, Outcome, RunError},
    stats::Statistics,
    store::{Health, Measurement, Record, Store, StoreError, Turn, TurnStatus},
};

pub const NO_CURRENT_THREAD: &str = "no current thread; start one with `ask new`";

pub struct Query<'a> {
    pub mode: Mode,
    pub words: Option<&'a str>,
    pub stdin_is_terminal: bool,
    pub started: Instant,
}

/// The profile snapshot a query uses and, for a reply, the pinned thread.
struct Session {
    store: Store,
    target: Target,
    credential: String,
    thread: Option<i64>,
    history: Vec<Exchange>,
    retention: Retention,
}

/// The history expiry setting read when the command started.
enum Retention {
    Indefinite,
    Days(u64),
    /// A reply could not read the installed configuration; the cause is
    /// reported and history is kept.
    Unreadable(String),
}

impl From<&config::Config> for Retention {
    fn from(config: &config::Config) -> Self {
        config.history_days().map_or(Self::Indefinite, Self::Days)
    }
}

/// One finished or failed query, measured when streaming ended.
struct Finished<'a> {
    prompt: &'a str,
    outcome: Outcome,
    started_at: SystemTime,
    wall: Duration,
}

pub async fn run(
    query: Query<'_>,
    stdout: &mut impl io::Write,
    stderr: &mut impl io::Write,
) -> ExitCode {
    let started_at = SystemTime::now();
    let mut session = match prepare(query.mode) {
        Ok(session) => session,
        Err(message) => return report(stderr, &message, ExitCode::FAILURE),
    };
    let stdin = io::stdin();
    let prompt = match input::resolve(
        query.words,
        query.stdin_is_terminal,
        &mut stdin.lock(),
        stderr,
    ) {
        Ok(prompt) => prompt,
        Err(error) => return report(stderr, &error.to_string(), error.status()),
    };
    if let Err(message) = session.expire(stderr) {
        return report(stderr, &message, ExitCode::FAILURE);
    }
    let outcome = session.ask(&prompt, stdout).await;
    let finished = Finished {
        prompt: &prompt,
        outcome,
        started_at,
        wall: query.started.elapsed(),
    };
    let saved = session.save(&finished);
    conclude(stderr, &session.target, &finished, saved)
}

fn prepare(mode: Mode) -> Result<Session, String> {
    match mode {
        Mode::New => fresh(),
        Mode::Reply => continued(),
    }
}

fn fresh() -> Result<Session, String> {
    let config = config::load().map_err(|error| error.to_string())?;
    let retention = Retention::from(&config);
    let target = config.resolve().map_err(|error| error.to_string())?;
    let credential = credential(&target.api_key_env)?;
    Ok(Session {
        store: open()?,
        target,
        credential,
        thread: None,
        history: Vec::new(),
        retention,
    })
}

/// A reply needs only the database and the snapshot's credential variable.
/// It reads the installed configuration only for history expiry: when that
/// configuration is missing or invalid, history is kept with a warning. The
/// thread is the one current when the command starts.
fn continued() -> Result<Session, String> {
    let retention = config::load().map_or_else(
        |error| Retention::Unreadable(error.to_string()),
        |config| Retention::from(&config),
    );
    let mut store = open()?;
    let thread = store
        .current()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| NO_CURRENT_THREAD.to_string())?;
    let credential = credential(&thread.target.api_key_env)?;
    Ok(Session {
        store,
        target: thread.target,
        credential,
        thread: Some(thread.id),
        history: thread.history,
        retention,
    })
}

fn open() -> Result<Store, String> {
    let path = config::data_path().map_err(|error| error.to_string())?;
    Store::open(&path).map_err(|error| error.to_string())
}

impl Session {
    /// Applies history expiry once valid input is submitted, before the
    /// request. A reply whose thread no longer exists sends nothing.
    fn expire(&mut self, stderr: &mut impl io::Write) -> Result<(), String> {
        match &self.retention {
            Retention::Indefinite => {}
            Retention::Days(days) => {
                self.store
                    .expire(SystemTime::now(), *days)
                    .map_err(|error| format!("cannot apply history expiry: {error}"))?;
            }
            Retention::Unreadable(cause) => {
                report(
                    stderr,
                    &format!("history expiry skipped: {cause}"),
                    ExitCode::SUCCESS,
                );
            }
        }
        match self.thread {
            Some(id)
                if !self
                    .store
                    .has_thread(id)
                    .map_err(|error| error.to_string())? =>
            {
                Err(NO_CURRENT_THREAD.to_string())
            }
            _ => Ok(()),
        }
    }

    async fn ask(&self, prompt: &str, stdout: &mut impl io::Write) -> Outcome {
        let provider = RigProvider::new(&self.target, self.credential.clone());
        let request = Request {
            prompt,
            system_prompt: &self.target.system_prompt,
            history: &self.history,
        };
        let mut answer = AnswerWriter::new(stdout);
        runner::run(&provider, &self.target, request, &mut answer).await
    }

    fn save(&mut self, finished: &Finished<'_>) -> Result<(), StoreError> {
        let turn = finished.status().map(|status| Turn {
            prompt: finished.prompt,
            answer: &finished.outcome.answer,
            status,
        });
        let command = if self.thread.is_some() {
            "reply"
        } else {
            "new"
        };
        let record = Record {
            started_at: finished.started_at,
            target: &self.target,
            thread: self.thread,
            measurement: finished.measurement(command, turn.as_ref()),
            turn,
            health: finished.health(),
        };
        self.store.record(&record)
    }
}

impl Finished<'_> {
    /// `None` when the provider failed before any answer text: nothing is
    /// appended and no thread is created.
    fn status(&self) -> Option<TurnStatus> {
        match &self.outcome.error {
            None => Some(TurnStatus::Complete),
            Some(error) if error.is_broken_pipe() => {
                Some(TurnStatus::Partial("output closed".to_string()))
            }
            Some(error) if self.has_partial_turn(error) => {
                Some(TurnStatus::Partial(error.to_string()))
            }
            Some(_) => None,
        }
    }

    fn has_partial_turn(&self, error: &RunError) -> bool {
        error.is_output() || error.is_output_limit() || !self.outcome.answer.is_empty()
    }

    /// Output failures say nothing about the provider target; an answer cut at
    /// the output-token limit shows the target responded normally.
    fn health(&self) -> Option<Health> {
        match &self.outcome.error {
            None => Some(Health::Success),
            Some(error) if error.is_output_limit() => Some(Health::Success),
            Some(error) if error.is_output() => None,
            Some(error) => Some(Health::Failure(error.class())),
        }
    }

    fn measurement(&self, command: &'static str, turn: Option<&Turn<'_>>) -> Measurement {
        let outcome = match turn.map(|turn| &turn.status) {
            Some(TurnStatus::Complete) => "complete",
            Some(TurnStatus::Partial(_)) => "partial",
            None => "failed",
        };
        Measurement {
            command,
            outcome,
            error_class: self.outcome.error.as_ref().map(|error| error.class()),
            wall: self.wall,
            api: self.outcome.api,
            first_token: self.outcome.first_token,
            usage: self.outcome.usage,
        }
    }

    fn statistics(&self, target: &Target) -> Statistics {
        Statistics {
            model: target.model.clone(),
            wall: self.wall,
            api: self.outcome.api,
            ttft: self.outcome.first_token.unwrap_or(self.outcome.api),
            usage: self.outcome.usage,
        }
    }
}

fn conclude(
    stderr: &mut impl io::Write,
    target: &Target,
    finished: &Finished<'_>,
    saved: Result<(), StoreError>,
) -> ExitCode {
    let error = finished.outcome.error.as_ref();
    if let Some(error) = error.filter(|error| !error.is_broken_pipe()) {
        let message = if error.is_output_limit() {
            format!("warning: {error}")
        } else {
            error.to_string()
        };
        report(stderr, &message, ExitCode::FAILURE);
    }
    if let Err(cause) = saved {
        let what = match finished.status() {
            Some(_) => "answer was delivered but not recorded",
            None => "query statistics were not recorded",
        };
        return report(stderr, &format!("{what}: {cause}"), ExitCode::FAILURE);
    }
    match error {
        None => report(
            stderr,
            &finished.statistics(target).to_string(),
            ExitCode::SUCCESS,
        ),
        Some(error) if error.is_broken_pipe() => ExitCode::SUCCESS,
        Some(_) => ExitCode::FAILURE,
    }
}
