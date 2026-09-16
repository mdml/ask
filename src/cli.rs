use std::{fmt, path::PathBuf};

const USAGE: &str = "usage: ask [new|n] <prompt words...> | ask [reply|r] <prompt words...> | ask [thread|t] | ask [switch|s] [ID] | ask stats | ask [init|i] | ask [configure|c] check [FILE|-] | ask [configure|c] apply [FILE|-]";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Query(Mode, Option<String>),
    Thread,
    /// Select the current thread, interactively when no id is given.
    Switch(Option<i64>),
    Stats,
    Init,
    Configure(Action),
}

/// Whether a query starts a new thread or continues the current one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    New,
    Reply,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Check(Source),
    Apply(Source),
}

/// Where a complete candidate configuration document is read from.
#[derive(Debug, PartialEq, Eq)]
pub enum Source {
    Stdin,
    File(PathBuf),
}

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdin => formatter.write_str("standard input"),
            Self::File(path) => write!(formatter, "'{}'", path.display()),
        }
    }
}

/// Parses the argument list. `stdin_is_terminal` decides whether an omitted
/// `FILE` argument can fall back to redirected standard input.
pub fn parse(
    args: impl IntoIterator<Item = String>,
    stdin_is_terminal: bool,
) -> Result<Command, String> {
    let mut words: Vec<String> = args.into_iter().collect();
    let (mode, named) = match words.first().map(String::as_str) {
        Some("init" | "i") => return alone(&words, Command::Init),
        Some("thread" | "t") => return alone(&words, Command::Thread),
        Some("stats") => return alone(&words, Command::Stats),
        Some("switch" | "s") => return switch(&words[1..]),
        Some("configure" | "c") => return configure(&words[1..], stdin_is_terminal),
        Some("new" | "n") => (Mode::New, true),
        Some("reply" | "r") => (Mode::Reply, true),
        _ => (Mode::New, false),
    };
    if named {
        words.remove(0);
    }
    Ok(Command::Query(
        mode,
        (!words.is_empty()).then(|| words.join(" ")),
    ))
}

/// A command that takes no arguments.
fn alone(words: &[String], command: Command) -> Result<Command, String> {
    if words.len() == 1 {
        return Ok(command);
    }
    Err(USAGE.to_string())
}

fn switch(rest: &[String]) -> Result<Command, String> {
    match rest {
        [] => Ok(Command::Switch(None)),
        [id] if id.bytes().all(|byte| byte.is_ascii_digit()) => match id.parse::<i64>() {
            Ok(id) if id > 0 => Ok(Command::Switch(Some(id))),
            _ => Err(USAGE.to_string()),
        },
        _ => Err(USAGE.to_string()),
    }
}

fn configure(rest: &[String], stdin_is_terminal: bool) -> Result<Command, String> {
    let (verb, rest) = rest.split_first().ok_or_else(|| USAGE.to_string())?;
    let action = match verb.as_str() {
        "check" => Action::Check(source(rest, stdin_is_terminal)?),
        "apply" => Action::Apply(source(rest, stdin_is_terminal)?),
        _ => return Err(USAGE.to_string()),
    };
    Ok(Command::Configure(action))
}

fn source(rest: &[String], stdin_is_terminal: bool) -> Result<Source, String> {
    match rest {
        [] if stdin_is_terminal => Err(format!(
            "reading a configuration from a terminal is not supported; name a file or redirect standard input\n{USAGE}"
        )),
        [] => Ok(Source::Stdin),
        [one] if one == "-" => Ok(Source::Stdin),
        [one] => Ok(Source::File(PathBuf::from(one))),
        _ => Err(USAGE.to_string()),
    }
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
