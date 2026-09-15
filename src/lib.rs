//! Query execution for the `ask` binary.

mod cli;
mod config;
mod configure;
mod init;
mod input;
mod output;
mod provider;
mod query;
mod runner;
mod stats;
mod store;
mod validate;

use std::{
    env,
    io::{self, IsTerminal},
    process::ExitCode,
    time::Instant,
};

pub const DEFAULT_SYSTEM_PROMPT: &str = "Answer briefly in plain Markdown suitable for a terminal.";

pub async fn run(args: impl IntoIterator<Item = String>) -> ExitCode {
    let stdout = io::stdout();
    let stderr = io::stderr();
    execute(args, &mut stdout.lock(), &mut stderr.lock()).await
}

async fn execute(
    args: impl IntoIterator<Item = String>,
    stdout: &mut impl io::Write,
    stderr: &mut impl io::Write,
) -> ExitCode {
    let wall_start = Instant::now();
    let stdin_is_terminal = io::stdin().is_terminal();
    let (mode, words) = match cli::parse(args, stdin_is_terminal) {
        Ok(cli::Command::Query(mode, words)) => (mode, words),
        Ok(cli::Command::Init) => return init(stderr),
        Ok(cli::Command::Configure(action)) => return configure(&action, stderr),
        Err(message) => return report(stderr, &message, ExitCode::from(2)),
    };
    let query = query::Query {
        mode,
        words: words.as_deref(),
        stdin_is_terminal,
        started: wall_start,
    };
    query::run(query, stdout, stderr).await
}

fn init(stderr: &mut impl io::Write) -> ExitCode {
    let path = match config::config_path() {
        Ok(path) => path,
        Err(error) => return report(stderr, &error.to_string(), ExitCode::FAILURE),
    };
    match init::run(&path, &mut io::stdin().lock(), stderr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report(stderr, &error.to_string(), ExitCode::FAILURE),
    }
}

fn configure(action: &cli::Action, stderr: &mut impl io::Write) -> ExitCode {
    let mut stdin = io::stdin().lock();
    let outcome = match action {
        cli::Action::Check(source) => configure::check(source, &mut stdin),
        cli::Action::Apply(source) => match config::config_path() {
            Ok(path) => configure::apply(&path, source, &mut stdin),
            Err(error) => Err(error.to_string()),
        },
    };
    match outcome {
        Ok(message) => report(stderr, &message, ExitCode::SUCCESS),
        Err(message) => report(stderr, &message, ExitCode::FAILURE),
    }
}

fn credential(name: &str) -> Result<String, String> {
    env::var(name).map_err(|error| match error {
        env::VarError::NotPresent => format!("credential environment variable '{name}' is not set"),
        env::VarError::NotUnicode(_) => {
            format!("credential environment variable '{name}' is not valid Unicode")
        }
    })
}

fn report(stderr: &mut impl io::Write, message: &str, status: ExitCode) -> ExitCode {
    let message = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let _ = writeln!(stderr, "ask: {message}");
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_collapses_multiline_diagnostics() {
        let mut output = Vec::new();
        let status = report(&mut output, "bad\n  input", ExitCode::FAILURE);
        assert_eq!(status, ExitCode::FAILURE);
        assert_eq!(output, b"ask: bad input\n");
    }
}
