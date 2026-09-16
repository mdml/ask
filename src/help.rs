//! Offline command reference and version string.

use std::{io, process::ExitCode};

use crate::emit;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const TEXT: &str = "\
ask — fast terminal lookup for language models

Usage:
  ask [--profile NAME | -p NAME] [new|n] [prompt words...]
  ask [reply|r] [prompt words...]
  ask [thread|t]
  ask [switch|s] [ID]
  ask stats
  ask [init|i]
  ask [configure|c] check [FILE|-]
  ask [configure|c] apply [FILE|-]
  ask help
  ask version

Bare ask (no subcommand) is the same as ask new: it starts a new thread.
Query commands start a new thread or continue the current one with reply.
Use --profile or -p on a new query to override the configured default profile.
Replies use the profile captured when their thread was created; --profile on reply
is rejected.

Prompt words join with single spaces; without them, stdin supplies the prompt.

Initialization:
  ask init creates the first configuration interactively. Supported providers
  supply endpoint and credential-variable defaults; model identifiers are free text.
  Credentials are read from environment variables only, scoped to the ask process.
  Use a hidden prompt or your existing credential manager to inject them only when
  launching ask. See the README and docs/guides/credentials.md for recipes.

Configuration:
  ask configure check validates a complete TOML document without writing.
  ask configure apply installs a validated document atomically.

Supported provider kinds: openai, anthropic, gemini, openrouter, openai-compatible.

See the project README for the full configuration schema and shell-composition rules.
";

pub fn run(stdout: &mut impl io::Write, stderr: &mut impl io::Write) -> ExitCode {
    emit(stdout, stderr, TEXT)
}

pub fn version(stdout: &mut impl io::Write, stderr: &mut impl io::Write) -> ExitCode {
    emit(stdout, stderr, &format!("ask {VERSION}\n"))
}
