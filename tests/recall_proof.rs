//! End-to-end proof of recall (`ask thread`, `ask switch`, `ask stats`) and
//! optional history expiry.
//!
//! Each command runs as a separate process of the real binary against a fake
//! provider. Proofs that depend on age or time rewrite the timestamps recorded
//! in the database to fixed values or to whole days before the present, so the
//! results do not depend on when the proof runs.

mod support;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::Connection;
use support::{
    command,
    fake_provider::{FakeProvider, Scenario},
    fresh_home,
};

const DAY_MS: i64 = 86_400_000;
const NO_CURRENT: &str = "ask: no current thread; start one with `ask new`\n";

/// The history settings a proof installs.
#[derive(Clone, Copy)]
enum Retention {
    /// The default: `expire_history` unset, history kept indefinitely.
    Indefinite,
    /// `expire_history = true` with the default `history_days`.
    DefaultDays,
    /// `expire_history = true` with an explicit `history_days`.
    Days(u32),
}

impl Retention {
    fn toml(self) -> String {
        match self {
            Self::Indefinite => String::new(),
            Self::DefaultDays => "expire_history = true\n".to_string(),
            Self::Days(days) => format!("expire_history = true\nhistory_days = {days}\n"),
        }
    }
}

#[test]
fn thread_shows_the_full_current_thread_with_incomplete_turns() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Stream,
        Scenario::PartialFailure,
        Scenario::Answer("third answer"),
    ]);
    let home = configured(&fake, Retention::Indefinite);
    succeeded(&ask(&home, &["new", "first"]));
    assert_eq!(ask(&home, &["r", "second\nline"]).status.code(), Some(1));
    succeeded(&ask(&home, &["r", "third"]));
    let expected = "thread 1 · profile terse · model fake-model\n\n\
                    You:\nfirst\n\nAssistant:\n**4**\n\n\
                    You:\nsecond\nline\n\nAssistant:\npartial\n[incomplete: provider request failed: ";
    for name in ["thread", "t"] {
        let output = ask(&home, &[name]);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(output.status.success() && output.stderr.is_empty());
        assert!(stdout.starts_with(expected), "{stdout}");
        assert!(
            stdout.ends_with("]\n\nYou:\nthird\n\nAssistant:\nthird answer\n"),
            "{stdout}"
        );
    }
}

#[test]
fn thread_without_a_current_thread_fails_and_creates_no_database() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Indefinite);
    let output = ask(&home, &["thread"]);
    assert_eq!(
        (output.status.code(), &output.stdout[..], stderr(&output)),
        (Some(1), &b""[..], NO_CURRENT.to_string())
    );
    assert!(!home.join("data").exists());
    let usage = ask(&home, &["t", "extra"]);
    assert_eq!(usage.status.code(), Some(2));
}

#[test]
fn thread_output_survives_an_early_closed_reader() {
    let fake = FakeProvider::start(Scenario::Answer("short"));
    let home = configured(&fake, Retention::Indefinite);
    succeeded(&ask(&home, &["new", "q"]));
    let long = "y".repeat(1 << 20);
    execute(&home, &[&format!("UPDATE turns SET answer = '{long}'")]);
    let mut child = command(&home, false)
        .arg("thread")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "{output:?}"
    );
}

#[test]
fn switch_selects_by_id_or_menu_and_replies_follow_the_selection() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Answer("a1"),
        Scenario::Answer("a2"),
        Scenario::Answer("a3"),
        Scenario::Answer("r1"),
        Scenario::Answer("r2"),
    ]);
    let home = configured(&fake, Retention::Indefinite);
    for prompt in ["one", "two", "three"] {
        succeeded(&ask(&home, &["new", prompt]));
    }
    set_turn_times(&home, &[(1, 0), (2, 60_000), (3, 120_000)]);
    let switched = ask(&home, &["switch", "1"]);
    assert_eq!(
        (switched.status.code(), stderr(&switched)),
        (Some(0), "ask: current thread is now 1\n".to_string())
    );
    assert!(switched.stdout.is_empty());
    succeeded(&ask(&home, &["r", "back to one"]));
    assert_eq!(user_messages(&fake.requests(4)[3]), ["one", "back to one"]);
    let menu = piped(&home, &["s"], b"2\n");
    assert_eq!(menu.status.code(), Some(0), "{}", stderr(&menu));
    assert!(menu.stdout.is_empty());
    let listing = stderr(&menu);
    let lines: Vec<&str> = listing.lines().collect();
    assert!(
        lines[0].starts_with(" 1. thread 1 (current) · "),
        "{listing}"
    );
    assert!(
        lines[0].ends_with(" · 2 turns · terse · fake-model · one"),
        "{listing}"
    );
    assert_eq!(
        lines[1],
        " 2. thread 3 · 1970-01-01 00:02 UTC · 1 turn · terse · fake-model · three"
    );
    assert_eq!(
        lines[2],
        " 3. thread 2 · 1970-01-01 00:01 UTC · 1 turn · terse · fake-model · two"
    );
    assert_eq!(
        lines[3],
        "select a thread [1-3]: ask: current thread is now 3"
    );
    succeeded(&ask(&home, &["r", "back to three"]));
    assert_eq!(
        user_messages(&fake.requests(5)[4]),
        ["three", "back to three"]
    );
}

#[cfg(unix)]
#[test]
fn terminal_switch_handles_selection_escape_and_keyboard_interrupt_and_restores_the_terminal() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Answer("a1"),
        Scenario::Answer("a2"),
        Scenario::Answer("a3"),
    ]);
    let home = configured(&fake, Retention::Indefinite);
    for prompt in ["one", "two", "three"] {
        succeeded(&ask(&home, &["new", prompt]));
    }
    for scenario in ["select", "escape", "cancel"] {
        let output = std::process::Command::new("python3")
            .arg("tests/support/switch_menu_process.py")
            .arg(env!("CARGO_BIN_EXE_ask"))
            .arg(&home)
            .arg(scenario)
            .output()
            .unwrap();
        assert!(output.status.success(), "{scenario}: {}", stderr(&output));
        assert_rows(&home, &[("SELECT thread_id FROM current_thread", "2")]);
    }
    let output = std::process::Command::new("python3")
        .arg("tests/support/switch_menu_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(&home)
        .arg("dumb")
        .output()
        .unwrap();
    assert!(output.status.success(), "dumb: {}", stderr(&output));
}

#[cfg(unix)]
#[test]
fn terminal_switch_renders_the_selected_thread_when_current_changes_after_selection() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Indefinite);
    for prompt in ["one", "two", "three"] {
        succeeded(&ask(&home, &["new", prompt]));
    }
    let output = std::process::Command::new("python3")
        .arg("tests/support/switch_snapshot_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(&home)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
}

#[cfg(unix)]
#[test]
fn terminal_switch_bounds_a_wide_menu_and_keeps_wrapped_selection_visible() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Indefinite);
    for number in 1..=10 {
        succeeded(&ask(
            &home,
            &["new", &format!("{number}界界界界界界界界界界界界界界界界")],
        ));
    }
    let output = std::process::Command::new("python3")
        .arg("tests/support/switch_menu_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(&home)
        .arg("viewport")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let output = std::process::Command::new("python3")
        .arg("tests/support/switch_menu_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(&home)
        .arg("tiny")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn switch_rejects_unknown_ids_and_invalid_selections_without_changing_current() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let empty = configured(&fake, Retention::Indefinite);
    for args in [&["switch"][..], &["switch", "4"][..]] {
        assert_eq!(ask(&empty, args).status.code(), Some(1));
    }
    assert!(!empty.join("data").exists());
    let home = configured(&fake, Retention::Indefinite);
    succeeded(&ask(&home, &["new", "one"]));
    let unknown = ask(&home, &["s", "4"]);
    assert_eq!(
        (unknown.status.code(), stderr(&unknown)),
        (Some(1), "ask: no thread with id 4\n".to_string())
    );
    for answer in [&b""[..], b"\n", b"0\n", b"2\n", b"one\n"] {
        let output = piped(&home, &["switch"], answer);
        assert_eq!(output.status.code(), Some(2));
        assert!(
            stderr(&output).ends_with(
                "ask: selection must be a number from 1 to 1; current thread unchanged\n"
            ),
            "{}",
            stderr(&output)
        );
    }
    assert_eq!(ask(&home, &["switch", "abc"]).status.code(), Some(2));
    assert_rows(&home, &[("SELECT thread_id FROM current_thread", "1")]);
}

#[test]
fn stats_reports_queries_history_and_historical_health_from_fixtures() {
    let home = fresh_home();
    let missing = ask(&home, &["stats"]);
    assert!(missing.status.success() && missing.stderr.is_empty());
    assert!(
        String::from_utf8(missing.stdout)
            .unwrap()
            .starts_with("queries: 0 · ")
    );
    assert!(!home.join("data").exists());
    let fake = FakeProvider::sequence(vec![
        Scenario::Stream,
        Scenario::Unauthorized,
        Scenario::StreamWithoutUsage,
    ]);
    install(&home, &fake, Retention::Indefinite);
    succeeded(&ask(&home, &["new", "PROMPT_SENTINEL"]));
    assert_eq!(ask(&home, &["r", "denied"]).status.code(), Some(1));
    succeeded(&ask(&home, &["r", "again"]));
    execute(
        &home,
        &[
            "UPDATE query_statistics SET wall_ms = id * 1000, first_token_ms = CASE WHEN outcome = 'complete' THEN id * 100 END",
            "UPDATE provider_health SET last_success_at_ms = 120000, last_failure_at_ms = 60000",
            "INSERT INTO history_expiry VALUES (1, 4, 0, 0)",
        ],
    );
    let output = ask(&home, &["stats"]);
    assert!(output.status.success() && output.stderr.is_empty());
    let expected = format!(
        "queries: 3 · 2 complete · 0 partial · 1 failed\n\
         tokens: 12 in / 3 out · reported by 1 query\n\
         median complete query: 1.0s wall · 0.1s to first token\n\
         history: 1 thread · 2 turns · 4 threads cleared by expiry\n\
         \n\
         provider targets (historical observations, not a current check):\n\
         openai-compatible · {} · fake-model\n  \
         3 queries · last observed healthy 1970-01-01 00:02 UTC (from query) · last failure 1970-01-01 00:01 UTC (provider) (from query)\n",
        fake.base_url()
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    assert_eq!(ask(&home, &["stats", "extra"]).status.code(), Some(2));
}

#[test]
fn history_is_kept_indefinitely_by_default() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Indefinite);
    succeeded(&ask(&home, &["new", "ancient"]));
    age_days(&home, 1, 10_000);
    succeeded(&ask(&home, &["new", "today"]));
    succeeded(&ask(&home, &["switch", "1"]));
    succeeded(&ask(&home, &["reply", "still here"]));
    assert_rows(&home, &[("SELECT count(*) FROM threads", "2")]);
}

#[test]
fn enabled_expiry_defaults_to_ninety_days_by_newest_turn() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::DefaultDays);
    for prompt in ["expired", "kept", "revived"] {
        succeeded(&ask(&home, &["new", prompt]));
    }
    succeeded(&ask(&home, &["switch", "3"]));
    succeeded(&ask(&home, &["reply", "newest turn"]));
    age_days(&home, 1, 91);
    age_days(&home, 2, 89);
    execute(
        &home,
        &[&format!(
            "UPDATE turns SET created_at_ms = {} WHERE thread_id = 3 AND ordinal = 1",
            epoch_now() - 200 * DAY_MS
        )],
    );
    for inspection in [&["thread"][..], &["stats"][..], &["switch", "1"][..]] {
        succeeded(&ask(&home, inspection));
    }
    assert_rows(&home, &[("SELECT count(*) FROM threads", "3")]);
    succeeded(&ask(&home, &["new", "trigger"]));
    assert_rows(
        &home,
        &[
            (
                "SELECT group_concat(id, ',') FROM (SELECT id FROM threads ORDER BY id)",
                "2,3,4",
            ),
            ("SELECT count(*) FROM turns WHERE thread_id = 3", "2"),
            ("SELECT count(*) FROM query_statistics", "5"),
            ("SELECT count(*) FROM provider_health", "1"),
            ("SELECT threads_cleared FROM history_expiry", "1"),
        ],
    );
    let stats = String::from_utf8(ask(&home, &["stats"]).stdout).unwrap();
    assert!(
        stats.contains("history: 3 threads · 4 turns · 1 thread cleared by expiry\n")
            && stats.starts_with("queries: 5 · 5 complete"),
        "{stats}"
    );
}

#[test]
fn a_reply_to_an_expired_current_thread_reports_no_current_thread() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Days(7));
    succeeded(&ask(&home, &["new", "q"]));
    age_days(&home, 1, 8);
    let output = ask(&home, &["reply", "follow up"]);
    assert_eq!(
        (output.status.code(), &output.stdout[..], stderr(&output)),
        (Some(1), &b""[..], NO_CURRENT.to_string())
    );
    assert_eq!(fake.requests(2).len(), 1);
    assert_rows(
        &home,
        &[
            ("SELECT count(*) FROM threads", "0"),
            ("SELECT count(*) FROM current_thread", "0"),
            ("SELECT count(*) FROM query_statistics", "1"),
            ("SELECT threads_cleared FROM history_expiry", "1"),
        ],
    );
    let thread = ask(&home, &["thread"]);
    assert_eq!(
        (thread.status.code(), stderr(&thread)),
        (Some(1), NO_CURRENT.to_string())
    );
}

#[test]
fn replies_keep_history_when_configuration_is_missing_or_invalid() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Days(7));
    succeeded(&ask(&home, &["new", "q"]));
    let installed = home.join("config.toml");
    for replacement in [Some("not = [valid"), None] {
        age_days(&home, 1, 30);
        match replacement {
            Some(text) => fs::write(&installed, text).unwrap(),
            None => fs::remove_file(&installed).unwrap(),
        }
        let output = ask(&home, &["reply", "captured profile"]);
        succeeded(&output);
        let warning = stderr(&output);
        let lines: Vec<&str> = warning.lines().collect();
        assert!(
            lines.len() == 2
                && lines[0].starts_with("ask: history expiry skipped: ")
                && lines[1].starts_with("ask: fake-model"),
            "{warning}"
        );
    }
    assert_rows(
        &home,
        &[
            ("SELECT count(*) FROM turns WHERE thread_id = 1", "3"),
            ("SELECT count(*) FROM history_expiry", "0"),
        ],
    );
}

#[test]
fn a_reply_with_valid_configuration_applies_its_expiry_to_other_threads() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Days(7));
    succeeded(&ask(&home, &["new", "old"]));
    succeeded(&ask(&home, &["new", "current"]));
    age_days(&home, 1, 8);
    succeeded(&ask(&home, &["reply", "more"]));
    assert_rows(
        &home,
        &[
            ("SELECT group_concat(id) FROM threads", "2"),
            ("SELECT threads_cleared FROM history_expiry", "1"),
        ],
    );
}

#[test]
fn cancelled_and_blank_queries_leave_history_unchanged() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Days(7));
    succeeded(&ask(&home, &["new", "old"]));
    succeeded(&ask(&home, &["new", "current"]));
    age_days(&home, 1, 30);
    for command in ["new", "reply"] {
        terminal(&home, "cancel", &[command]);
        terminal(&home, "blank", &[command]);
        assert_eq!(piped(&home, &[command], b" \n").status.code(), Some(2));
    }
    assert_eq!(fake.requests(3).len(), 2);
    assert_rows(
        &home,
        &[
            ("SELECT group_concat(id) FROM threads", "1,2"),
            ("SELECT count(*) FROM history_expiry", "0"),
            ("SELECT count(*) FROM query_statistics", "2"),
        ],
    );
}

#[test]
fn a_captured_thread_that_expires_when_input_is_submitted_sends_no_request() {
    let fake = FakeProvider::start(Scenario::Answer("a"));
    let home = configured(&fake, Retention::Days(7));
    succeeded(&ask(&home, &["new", "q"]));
    age_days(&home, 1, 8);
    terminal(&home, "expired", &["reply"]);
    assert_eq!(fake.requests(2).len(), 1);
    assert_rows(
        &home,
        &[
            ("SELECT count(*) FROM threads", "0"),
            ("SELECT threads_cleared FROM history_expiry", "1"),
        ],
    );
}

#[test]
fn a_reply_continues_the_thread_current_at_start_despite_a_switch_during_input() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Answer("a1"),
        Scenario::Answer("a2"),
        Scenario::Answer("r2"),
    ]);
    let home = configured(&fake, Retention::Days(7));
    succeeded(&ask(&home, &["new", "one"]));
    succeeded(&ask(&home, &["new", "two"]));
    terminal(&home, "switch:1", &["reply"]);
    assert_eq!(user_messages(&fake.requests(3)[2]), ["two", "follow up\n"]);
    assert_rows(
        &home,
        &[
            (
                "SELECT group_concat(thread_id || ':' || turns) FROM (SELECT thread_id, count(*) AS turns FROM turns GROUP BY thread_id ORDER BY thread_id)",
                "1:1,2:2",
            ),
            ("SELECT thread_id FROM current_thread", "2"),
        ],
    );
}

#[test]
fn a_reply_whose_thread_expires_while_streaming_records_nothing() {
    let fake = FakeProvider::holding(
        vec![
            Scenario::Answer("a1"),
            Scenario::Answer("r1"),
            Scenario::Answer("b1"),
        ],
        1,
    );
    let home = configured(&fake, Retention::Days(7));
    succeeded(&ask(&home, &["new", "qa"]));
    let reply = command(&home, true)
        .args(["reply", "PRIVATE_FOLLOW_UP"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert_eq!(fake.requests(2).len(), 2);
    age_days(&home, 1, 8);
    succeeded(&ask(&home, &["new", "qb"]));
    fake.release();
    let output = reply.wait_with_output().unwrap();
    let diagnostic = stderr(&output);
    assert_eq!(
        (output.status.code(), &output.stdout[..]),
        (Some(1), &b"r1\n"[..])
    );
    assert!(
        diagnostic.starts_with("ask: answer was delivered but not recorded: ")
            && diagnostic.lines().count() == 1
            && !diagnostic.contains("PRIVATE_FOLLOW_UP"),
        "{diagnostic}"
    );
    assert_rows(
        &home,
        &[
            ("SELECT group_concat(id) FROM threads", "2"),
            ("SELECT count(*) FROM turns", "1"),
            ("SELECT count(*) FROM query_statistics", "2"),
            (
                "SELECT group_concat(command) FROM query_statistics",
                "new,new",
            ),
            ("SELECT thread_id FROM current_thread", "2"),
            ("SELECT threads_cleared FROM history_expiry", "1"),
        ],
    );
}

/// Runs a query through a PTY with `tests/support/recall_process.py`.
fn terminal(home: &Path, scenario: &str, args: &[&str]) {
    let output = std::process::Command::new("python3")
        .arg("tests/support/recall_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(home)
        .arg(scenario)
        .args(args)
        .env("LOCAL_API_KEY", support::CREDENTIAL)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{scenario}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A fresh home with a configuration for the `terse` profile against `fake`.
fn configured(fake: &FakeProvider, retention: Retention) -> PathBuf {
    let home = fresh_home();
    install(&home, fake, retention);
    home
}

/// Writes the configuration for the `terse` profile against `fake` into `home`.
fn install(home: &Path, fake: &FakeProvider, retention: Retention) {
    let text = format!(
        "default_profile = \"terse\"\n{}\n[providers.local]\nkind = \"openai-compatible\"\nbase_url = \"{}\"\napi_key_env = \"LOCAL_API_KEY\"\n\n[profiles.terse]\nprovider = \"local\"\nmodel = \"fake-model\"\n",
        retention.toml(),
        fake.base_url()
    );
    fs::write(home.join("config.toml"), text).unwrap();
}

fn ask(home: &Path, args: &[&str]) -> Output {
    command(home, true)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn piped(home: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = command(home, true)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn succeeded(output: &Output) {
    assert!(output.status.success(), "{}", stderr(output));
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn user_messages(request: &support::fake_provider::RecordedRequest) -> Vec<&str> {
    request
        .messages
        .iter()
        .filter(|(role, _)| role == "user")
        .map(|(_, text)| text.as_str())
        .collect()
}

/// Milliseconds since the epoch now.
fn epoch_now() -> i64 {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    i64::try_from(ms).unwrap()
}

fn connection(home: &Path) -> Connection {
    Connection::open(home.join("data/ask.sqlite3")).unwrap()
}

fn execute(home: &Path, statements: &[&str]) {
    let connection = connection(home);
    for statement in statements {
        connection.execute_batch(statement).unwrap();
    }
}

/// Moves every turn of `thread` to `days` whole days before now.
fn age_days(home: &Path, thread: i64, days: i64) {
    let at = epoch_now() - days * DAY_MS;
    execute(
        home,
        &[&format!(
            "UPDATE turns SET created_at_ms = {at} WHERE thread_id = {thread}"
        )],
    );
}

/// Sets every turn of each thread to a fixed time in milliseconds.
fn set_turn_times(home: &Path, times: &[(i64, i64)]) {
    for (thread, at) in times {
        execute(
            home,
            &[&format!(
                "UPDATE turns SET created_at_ms = {at} WHERE thread_id = {thread}"
            )],
        );
    }
}

fn assert_rows(home: &Path, expectations: &[(&str, &str)]) {
    let connection = connection(home);
    for (query, expected) in expectations {
        let actual: Option<String> = connection
            .query_row(&format!("SELECT CAST(({query}) AS TEXT)"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(actual.as_deref(), Some(*expected), "{query}");
    }
}
