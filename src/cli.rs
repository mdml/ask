use std::{fmt, path::PathBuf};

const USAGE: &str = "usage: ask [--profile NAME | -p NAME] [new|n] [prompt words...] | ask [reply|r] [prompt words...] | ask [thread|t] | ask [switch|s] [ID] | ask stats | ask [init|i] | ask [configure|c] check [FILE|-] | ask [configure|c] apply [FILE|-] | ask help | ask version | ask [--help | -h] | ask [--version | -V]";

const REPLY_PROFILE: &str = "--profile and -p apply only to new queries; replies use the profile captured when their thread was created";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Query(Mode, QueryOptions),
    Thread,
    /// Select the current thread, interactively when no id is given.
    Switch(Option<i64>),
    Stats,
    Init,
    Configure(Action),
    Help,
    Version,
}

/// Options that apply only to a new query.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct QueryOptions {
    pub profile: Option<String>,
    pub words: Option<String>,
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
    let words: Vec<String> = args.into_iter().collect();
    match words.first().map(String::as_str) {
        Some("help" | "--help" | "-h") => alone(&words, Command::Help),
        Some("version" | "--version" | "-V") => alone(&words, Command::Version),
        Some("init" | "i") => alone(&words, Command::Init),
        Some("thread" | "t") => alone(&words, Command::Thread),
        Some("stats") => alone(&words, Command::Stats),
        Some("switch" | "s") => switch(&words[1..]),
        Some("configure" | "c") => configure(&words[1..], stdin_is_terminal),
        _ => parse_query(words),
    }
}

fn parse_query(mut words: Vec<String>) -> Result<Command, String> {
    let profile = leading_profile(&mut words, None)?;
    let mode = query_mode(&mut words);
    let profile = leading_profile(&mut words, profile)?;
    if profile.is_some() && mode == Mode::Reply {
        return Err(format!("{REPLY_PROFILE}\n{USAGE}"));
    }
    Ok(Command::Query(
        mode,
        QueryOptions {
            profile,
            words: (!words.is_empty()).then(|| words.join(" ")),
        },
    ))
}

fn query_mode(words: &mut Vec<String>) -> Mode {
    let mode = match words.first().map(String::as_str) {
        Some("new" | "n") => Mode::New,
        Some("reply" | "r") => Mode::Reply,
        _ => return Mode::New,
    };
    words.remove(0);
    mode
}

fn leading_profile(
    words: &mut Vec<String>,
    mut profile: Option<String>,
) -> Result<Option<String>, String> {
    while matches!(words.first().map(String::as_str), Some("--profile" | "-p")) {
        let flag = words.remove(0);
        profile = Some(flag_value(words, &flag)?);
    }
    Ok(profile)
}

fn flag_value(words: &mut Vec<String>, flag: &str) -> Result<String, String> {
    words
        .first()
        .filter(|value| !value.is_empty())
        .cloned()
        .inspect(|_| {
            words.remove(0);
        })
        .ok_or_else(|| format!("{flag} requires a profile name\n{USAGE}"))
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
