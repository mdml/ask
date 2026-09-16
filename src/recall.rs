//! `ask thread` shows the current thread; `ask switch` selects another one.

use std::{io, process::ExitCode};

use crate::{
    config, emit,
    query::NO_CURRENT_THREAD,
    report,
    store::{Store, ThreadSummary, ThreadView},
    utc,
};

/// How many threads the interactive `ask switch` offers.
const RECENT_THREADS: i64 = 10;
const OPENING_CHARS: usize = 60;

pub fn thread(stdout: &mut impl io::Write, stderr: &mut impl io::Write) -> ExitCode {
    let view = existing().and_then(|store| match store {
        Some(mut store) => store.current_view().map_err(|error| error.to_string()),
        None => Ok(None),
    });
    match view {
        Ok(Some(view)) => emit(stdout, stderr, &render(&view)),
        Ok(None) => report(stderr, NO_CURRENT_THREAD, ExitCode::FAILURE),
        Err(message) => report(stderr, &message, ExitCode::FAILURE),
    }
}

pub fn switch(
    id: Option<i64>,
    stdin: &mut impl io::BufRead,
    stderr: &mut impl io::Write,
) -> ExitCode {
    let outcome = existing()
        .map_err(Failure::Other)
        .and_then(|store| match (store, id) {
            (None, Some(id)) => Err(unknown(id).into()),
            (None, None) => Err(NO_THREADS.to_string().into()),
            (Some(mut store), Some(id)) => select(&mut store, id),
            (Some(mut store), None) => choose(&mut store, stdin, stderr),
        });
    match outcome {
        Ok(id) => report(
            stderr,
            &format!("current thread is now {id}"),
            ExitCode::SUCCESS,
        ),
        Err(Failure::Usage(message)) => report(stderr, &message, ExitCode::from(2)),
        Err(Failure::Other(message)) => report(stderr, &message, ExitCode::FAILURE),
    }
}

const NO_THREADS: &str = "no threads; start one with `ask new`";

enum Failure {
    Usage(String),
    Other(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

fn existing() -> Result<Option<Store>, String> {
    let path = config::data_path().map_err(|error| error.to_string())?;
    Store::open_existing(&path).map_err(|error| error.to_string())
}

fn unknown(id: i64) -> String {
    format!("no thread with id {id}")
}

fn select(store: &mut Store, id: i64) -> Result<i64, Failure> {
    match store.select(id) {
        Ok(true) => Ok(id),
        Ok(false) => Err(Failure::Other(unknown(id))),
        Err(error) => Err(Failure::Other(error.to_string())),
    }
}

/// Lists recent threads on stderr and reads one selection line from stdin.
fn choose(
    store: &mut Store,
    stdin: &mut impl io::BufRead,
    stderr: &mut impl io::Write,
) -> Result<i64, Failure> {
    let threads = store
        .recent(RECENT_THREADS)
        .map_err(|error| error.to_string())?;
    if threads.is_empty() {
        return Err(Failure::Other(NO_THREADS.to_string()));
    }
    let prompt = menu(&threads);
    let prompted = stderr
        .write_all(prompt.as_bytes())
        .and_then(|()| stderr.flush());
    prompted.map_err(|error| format!("cannot write selection prompt: {error}"))?;
    let mut line = String::new();
    stdin
        .read_line(&mut line)
        .map_err(|error| format!("cannot read selection: {error}"))?;
    let chosen = line
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|number| threads.get(number.checked_sub(1)?));
    match chosen {
        Some(thread) => select(store, thread.id),
        None => Err(Failure::Usage(format!(
            "selection must be a number from 1 to {}; current thread unchanged",
            threads.len()
        ))),
    }
}

fn menu(threads: &[ThreadSummary]) -> String {
    let mut text = String::new();
    for (index, thread) in threads.iter().enumerate() {
        text.push_str(&format!("{:>2}. {}\n", index + 1, entry(thread)));
    }
    text.push_str(&format!("select a thread [1-{}]: ", threads.len()));
    text
}

fn entry(thread: &ThreadSummary) -> String {
    let marker = if thread.current { " (current)" } else { "" };
    let turns = if thread.turns == 1 { "turn" } else { "turns" };
    format!(
        "thread {}{marker} · {} · {} {turns} · {} · {} · {}",
        thread.id,
        utc::format(thread.updated_at_ms),
        thread.turns,
        printable(&thread.profile),
        printable(&thread.model),
        opening(&thread.opening)
    )
}

/// Replaces control characters, which could move the cursor or restyle the
/// terminal, and invisible format characters, which could reorder or hide
/// what the entry shows, with spaces.
fn printable(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() || is_invisible_format(character) {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Unicode format characters that are invisible on their own: bidirectional
/// controls, zero-width spaces, soft hyphens, word joiners, interlinear
/// annotation, and tag characters. The zero-width joiner and non-joiner
/// (U+200C, U+200D) are kept because they shape emoji and scripts such as
/// Persian.
fn is_invisible_format(character: char) -> bool {
    matches!(
        character,
        '\u{ad}'
            | '\u{61c}'
            | '\u{180e}'
            | '\u{200b}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
    )
}

/// The first line of the thread's first prompt on one safe line.
fn opening(prompt: &str) -> String {
    let line = prompt.trim().lines().next().unwrap_or_default();
    let mut text: String = printable(line).chars().take(OPENING_CHARS).collect();
    if line.chars().count() > OPENING_CHARS {
        text.push('…');
    }
    text
}

/// The thread header, then each turn: the prompt quoted as Markdown and the
/// raw answer without trailing line endings. A partial turn ends with an
/// `[incomplete: <reason>]` marker line.
pub fn render(view: &ThreadView) -> String {
    let mut blocks = vec![format!(
        "thread {} · profile {} · model {}",
        view.id, view.profile, view.model
    )];
    for turn in &view.turns {
        blocks.push(quote(&turn.prompt));
        let mut lines: Vec<String> = Vec::new();
        let answer = turn.answer.trim_end_matches(['\r', '\n']);
        if !answer.is_empty() {
            lines.push(answer.to_string());
        }
        if let Some(reason) = &turn.reason {
            lines.push(format!("[incomplete: {reason}]"));
        }
        if !lines.is_empty() {
            blocks.push(lines.join("\n"));
        }
    }
    let mut text = blocks.join("\n\n");
    text.push('\n');
    text
}

fn quote(prompt: &str) -> String {
    prompt
        .trim_end_matches(['\r', '\n'])
        .lines()
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "recall_tests.rs"]
mod tests;
