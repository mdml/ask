//! `ask stats`: local query statistics and historical provider health.

use std::{io, process::ExitCode};

use crate::{
    config, emit, report,
    store::{Store, Summary, TargetHealth},
    utc,
};

pub fn run(stdout: &mut impl io::Write, stderr: &mut impl io::Write) -> ExitCode {
    match summary() {
        Ok(summary) => emit(stdout, stderr, &render(&summary)),
        Err(message) => report(stderr, &message, ExitCode::FAILURE),
    }
}

/// A missing database has recorded nothing, and is not created.
fn summary() -> Result<Summary, String> {
    let path = config::data_path().map_err(|error| error.to_string())?;
    match Store::open_existing(&path).map_err(|error| error.to_string())? {
        Some(mut store) => store.summary().map_err(|error| error.to_string()),
        None => Ok(Summary::default()),
    }
}

pub fn render(summary: &Summary) -> String {
    let mut lines = vec![
        format!(
            "queries: {} · {} complete · {} partial · {} failed",
            summary.queries, summary.complete, summary.partial, summary.failed
        ),
        format!(
            "tokens: {} in / {} out · reported by {}",
            summary.input_tokens,
            summary.output_tokens,
            counted(summary.with_usage, "query", "queries")
        ),
        format!(
            "median complete query: {} wall · {} to first token",
            seconds(summary.median_wall_ms),
            seconds(summary.median_first_token_ms)
        ),
        format!(
            "history: {} · {} · {} cleared by expiry",
            counted(summary.threads, "thread", "threads"),
            counted(summary.turns, "turn", "turns"),
            counted(summary.threads_cleared, "thread", "threads")
        ),
    ];
    if !summary.targets.is_empty() {
        lines.push(String::new());
        lines.push("provider targets (historical observations, not a current check):".to_string());
        lines.extend(summary.targets.iter().map(target));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn target(health: &TargetHealth) -> String {
    let healthy = health.last_success_at_ms.map_or_else(
        || "never observed healthy".to_string(),
        |at| {
            format!(
                "last observed healthy {} {}",
                utc::format(at),
                source_phrase(health.last_success_source.as_deref())
            )
        },
    );
    let failure = match (health.last_failure_at_ms, &health.last_failure_class) {
        (Some(at), Some(class)) => format!(
            "last failure {} ({class}) {}",
            utc::format(at),
            source_phrase(health.last_failure_source.as_deref())
        ),
        (Some(at), None) => format!(
            "last failure {} {}",
            utc::format(at),
            source_phrase(health.last_failure_source.as_deref())
        ),
        (None, _) => "no failures observed".to_string(),
    };
    format!(
        "{} · {} · {}\n  {} · {healthy} · {failure}",
        health.kind,
        health.base_url,
        health.model,
        counted(health.queries, "query", "queries")
    )
}

fn counted(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn source_phrase(source: Option<&str>) -> String {
    match source {
        Some("query") => "(from query)".to_string(),
        Some("live-check") => "(from live check)".to_string(),
        Some(other) => format!("(from {other})"),
        None => String::new(),
    }
}

fn seconds(milliseconds: Option<i64>) -> String {
    milliseconds.map_or_else(
        || "-".to_string(),
        |ms| format!("{}.{}s", ms / 1_000, ms % 1_000 / 100),
    )
}

#[cfg(test)]
#[path = "overview_tests.rs"]
mod tests;
