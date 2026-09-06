//! Query execution for the `ask` binary.

mod cli;
mod config;
mod configure;
mod output;
mod provider;
mod runner;
mod stats;

use std::{env, io, process::ExitCode, time::Instant};

use output::AnswerWriter;
use provider::RigProvider;

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
    let prompt = match cli::parse(args) {
        Ok(cli::Command::Query(prompt)) => prompt,
        Ok(cli::Command::Configure) => return configure(stderr),
        Err(message) => return report(stderr, &message, ExitCode::from(2)),
    };
    let target = match config::load().and_then(config::Config::resolve) {
        Ok(target) => target,
        Err(error) => return report(stderr, &error.to_string(), ExitCode::FAILURE),
    };
    let credential = match credential(&target.api_key_env) {
        Ok(value) => value,
        Err(message) => return report(stderr, &message, ExitCode::FAILURE),
    };
    let provider = RigProvider::new(&target, credential);
    let mut answer = AnswerWriter::new(stdout);
    match runner::run(&provider, &target, &prompt, wall_start, &mut answer).await {
        Ok(statistics) => report(stderr, &statistics.to_string(), ExitCode::SUCCESS),
        Err(error) if error.is_broken_pipe() => ExitCode::SUCCESS,
        Err(error) => report(stderr, &error.to_string(), ExitCode::FAILURE),
    }
}

fn configure(stderr: &mut impl io::Write) -> ExitCode {
    let path = match config::config_path() {
        Ok(path) => path,
        Err(error) => return report(stderr, &error.to_string(), ExitCode::FAILURE),
    };
    match configure::run(&path, &mut io::stdin().lock(), stderr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report(stderr, &error.to_string(), ExitCode::FAILURE),
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
