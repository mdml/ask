//! End-to-end proof that `ask reply` continues the current thread.
//!
//! Each command runs as a separate process of the real binary. `ask new`
//! records a thread with a snapshot of its resolved profile; a later
//! `ask reply` sends that snapshot's model and system prompt with the thread's
//! complete turns, which the fake provider records request by request.

mod support;

use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use rusqlite::Connection;
use support::{
    CREDENTIAL, command,
    fake_provider::{FakeProvider, RecordedRequest, Scenario},
    fresh_home,
};

/// The raw text the `Stream` scenario returns, before stdout normalization.
const RAW_ANSWER: &str = "**4**\n\n";
const TERSE: &str = "Use terse tables.";

#[test]
fn replies_send_prior_turns_in_order_with_the_snapshot() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Stream,
        Scenario::Answer("second answer"),
        Scenario::Answer("third"),
    ]);
    let home = configured(&fake.base_url());
    answered(&ask(&home, &["new", "first", "question"]), b"**4**\n");
    answered(
        &ask(&home, &["reply", "second", "question"]),
        b"second answer\n",
    );
    answered(&ask(&home, &["r", "third"]), b"third\n");
    let requests = fake.requests(3);
    assert_snapshot(&requests);
    let turns = [
        "first question",
        RAW_ANSWER,
        "second question",
        "second answer",
        "third",
    ];
    assert_eq!(requests[1].messages, conversation(&turns[..3]));
    assert_eq!(requests[2].messages, conversation(&turns));
}

#[test]
fn reply_keeps_the_snapshot_when_configuration_changes_or_disappears() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured(&fake.base_url());
    answered(&ask(&home, &["question"]), b"**4**\n");
    let changed = config_text("http://127.0.0.1:1/v1")
        .replace("terse", "other")
        .replace("fake-model", "other-model")
        .replace(TERSE, "Other prompt.");
    let installed = home.join("config.toml");
    fs::write(&installed, changed).unwrap();
    answered(&ask(&home, &["r", "after", "change"]), b"**4**\n");
    fs::write(&installed, "not = [valid").unwrap();
    answered(&ask(&home, &["r", "after", "invalid"]), b"**4**\n");
    fs::remove_file(&installed).unwrap();
    answered(&ask(&home, &["r", "after", "removal"]), b"**4**\n");
    assert_snapshot(&fake.requests(4));
    assert_rows(
        &home,
        &[
            ("SELECT count(*) FROM turns", "4"),
            (
                "SELECT group_concat(DISTINCT profile) FROM query_statistics",
                "terse",
            ),
        ],
    );
}

#[test]
fn new_forms_start_fresh_threads() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Answer("a1"),
        Scenario::Answer("a2"),
        Scenario::Answer("a3"),
        Scenario::Answer("a4"),
    ]);
    let home = configured(&fake.base_url());
    answered(&ask(&home, &["q1"]), b"a1\n");
    answered(&ask(&home, &["reply", "q2"]), b"a2\n");
    answered(&ask(&home, &["n", "q3"]), b"a3\n");
    answered(&ask(&home, &["r", "q4"]), b"a4\n");
    let requests = fake.requests(4);
    assert_eq!(requests[1].messages, conversation(&["q1", "a1", "q2"]));
    assert_eq!(requests[3].messages, conversation(&["q3", "a3", "q4"]));
    assert_rows(&home, &[("SELECT count(*) FROM threads", "2")]);
}

#[test]
fn an_overlapped_reply_appends_to_its_pinned_thread_and_finishes_current() {
    let fake = FakeProvider::holding(
        vec![
            Scenario::Answer("a1"),
            Scenario::Answer("r1"),
            Scenario::Answer("b1"),
            Scenario::Answer("r2"),
        ],
        1,
    );
    let home = configured(&fake.base_url());
    answered(&ask(&home, &["new", "qa"]), b"a1\n");
    let reply = command(&home, true)
        .args(["reply", "follow"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert_eq!(
        fake.requests(2)[1].messages,
        conversation(&["qa", "a1", "follow"])
    );
    answered(&ask(&home, &["new", "qb"]), b"b1\n");
    assert_rows(&home, &[("SELECT thread_id FROM current_thread", "2")]);
    fake.release();
    answered(&reply.wait_with_output().unwrap(), b"r1\n");
    assert_rows(
        &home,
        &[
            (
                "SELECT group_concat(thread_id || ':' || turns, ',') FROM (SELECT thread_id, count(*) AS turns FROM turns GROUP BY thread_id ORDER BY thread_id)",
                "1:2,2:1",
            ),
            ("SELECT thread_id FROM current_thread", "1"),
        ],
    );
    answered(&ask(&home, &["r", "again"]), b"r2\n");
    assert_eq!(
        fake.requests(4)[3].messages,
        conversation(&["qa", "a1", "follow", "r1", "again"])
    );
}

#[test]
fn reply_accepts_piped_input() {
    let fake = FakeProvider::start(Scenario::Answer("ok"));
    let home = configured(&fake.base_url());
    answered(&ask(&home, &["q1"]), b"ok\n");
    answered(&piped(&home, &["r", "summarize"], b"payload\n"), b"ok\n");
    answered(&piped(&home, &["reply"], b"stdin only"), b"ok\n");
    assert_eq!(
        fake.requests(3)[2].messages,
        conversation(&["q1", "ok", "summarize\n\npayload\n", "ok", "stdin only"])
    );
}

#[test]
fn reply_without_a_current_thread_sends_no_request() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = fresh_home();
    for args in [&["reply", "question"][..], &["r"][..]] {
        let stderr = failed_with(&ask(&home, args), b"");
        assert_eq!(stderr, "ask: no current thread; start one with `ask new`\n");
    }
    assert!(fake.recorded().is_none());
}

#[test]
fn failure_before_answer_text_leaves_the_current_thread() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Stream,
        Scenario::Unauthorized,
        Scenario::Unauthorized,
        Scenario::Answer("again"),
    ]);
    let home = configured(&fake.base_url());
    answered(&ask(&home, &["q1"]), b"**4**\n");
    for args in [&["new", "q2"][..], &["reply", "q3"][..]] {
        let stderr = failed_with(&ask(&home, args), b"");
        assert!(stderr.contains("401"), "{stderr}");
    }
    answered(&ask(&home, &["r", "q4"]), b"again\n");
    assert_eq!(
        fake.requests(4)[3].messages,
        conversation(&["q1", RAW_ANSWER, "q4"])
    );
    assert_rows(
        &home,
        &[
            ("SELECT count(*) FROM threads", "1"),
            (
                "SELECT group_concat(command || ':' || outcome, ',') FROM query_statistics",
                "new:complete,new:failed,reply:failed,reply:complete",
            ),
            (
                "SELECT (last_success_at_ms IS NOT NULL) || (last_failure_at_ms IS NOT NULL) || last_failure_class FROM provider_health",
                "11provider",
            ),
        ],
    );
}

#[test]
fn partial_turns_are_recorded_but_not_replayed() {
    let fake = FakeProvider::sequence(vec![Scenario::PartialFailure, Scenario::Answer("next")]);
    let home = configured(&fake.base_url());
    failed_with(&ask(&home, &["new", "q1"]), b"partial\n");
    answered(&ask(&home, &["reply", "q2"]), b"next\n");
    assert_eq!(fake.requests(2)[1].messages, conversation(&["q2"]));
    assert_rows(
        &home,
        &[
            (
                "SELECT group_concat(ordinal || status || ':' || answer, ',') FROM turns",
                "1partial:partial,2complete:next",
            ),
            (
                "SELECT reason LIKE 'provider request failed%' FROM turns WHERE ordinal = 1",
                "1",
            ),
        ],
    );
}

#[test]
fn empty_answers_are_complete_turns() {
    let fake = FakeProvider::sequence(vec![Scenario::Empty, Scenario::Answer("ok")]);
    let home = configured(&fake.base_url());
    answered(&ask(&home, &["q1"]), b"\n");
    answered(&ask(&home, &["r", "q2"]), b"ok\n");
    assert_eq!(
        fake.requests(2)[1].messages,
        conversation(&["q1", "", "q2"])
    );
}

#[test]
fn early_pipe_closure_records_a_partial_turn_quietly() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured(&fake.base_url());
    let mut child = command(&home, true)
        .arg("question")
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
    assert_rows(
        &home,
        &[
            (
                "SELECT status || ':' || reason FROM turns",
                "partial:output closed",
            ),
            ("SELECT count(*) FROM current_thread", "1"),
        ],
    );
}

#[test]
fn a_delivered_answer_that_cannot_be_recorded_fails_and_keeps_stdout() {
    let fake = FakeProvider::gated(Scenario::Stream);
    let home = configured(&fake.base_url());
    let child = command(&home, true)
        .args(["new", "PRIVATE_PROMPT"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert_eq!(fake.requests(1).len(), 1);
    let data = home.join("data");
    let database = data.join("ask.sqlite3");
    let before = fs::read(&database).unwrap();
    let writable = lock_down(&data, &database);
    fake.release();
    let output = child.wait_with_output().unwrap();
    set_mode(&data, 0o700);
    set_mode(&database, 0o600);
    if writable {
        eprintln!("skipped: permissions do not restrict this user (running as root?)");
        return;
    }
    let stderr = failed_with(&output, b"**4**\n");
    assert!(
        stderr.starts_with("ask: answer was delivered but not recorded: ")
            && stderr.lines().count() == 1
            && !stderr.contains("PRIVATE_PROMPT"),
        "{stderr}"
    );
    assert_eq!((fs::read(&database).unwrap(), entries(&data)), (before, 1));
}

#[test]
fn history_lives_in_the_data_directory_without_credentials() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured(&fake.base_url());
    let check = command(&home, false)
        .args(["configure", "check", "config.toml"])
        .current_dir(&home)
        .output()
        .unwrap();
    assert!(check.status.success() && !home.join("data").exists());
    answered(&ask(&home, &["question"]), b"**4**\n");
    let data = home.join("data");
    let database = data.join("ask.sqlite3");
    assert_eq!(
        (mode(&data), mode(&database), entries(&data)),
        (0o700, 0o600, 1)
    );
    let bytes = fs::read(&database).unwrap();
    assert!(
        !bytes
            .windows(CREDENTIAL.len())
            .any(|part| part == CREDENTIAL.as_bytes())
    );
    assert_rows(
        &home,
        &[("SELECT api_key_env FROM threads", "LOCAL_API_KEY")],
    );
}

#[cfg(target_os = "linux")]
#[test]
fn platform_data_directory_is_used_without_ask_home() {
    let home = fresh_home();
    let output = command(&home, true)
        .args(["reply", "question"])
        .env_remove("ASK_HOME")
        .env("HOME", &home)
        .env("XDG_DATA_HOME", home.join("xdg-data"))
        .output()
        .unwrap();
    failed_with(&output, b"");
    assert!(home.join("xdg-data/ask/ask.sqlite3").exists());
}

fn configured(base_url: &str) -> PathBuf {
    let home = fresh_home();
    fs::write(home.join("config.toml"), config_text(base_url)).unwrap();
    home
}

fn config_text(base_url: &str) -> String {
    format!(
        "default_profile = \"terse\"\n\n[providers.local]\nkind = \"openai-compatible\"\nbase_url = \"{base_url}\"\napi_key_env = \"LOCAL_API_KEY\"\n\n[profiles.terse]\nprovider = \"local\"\nmodel = \"fake-model\"\nsystem_prompt = \"{TERSE}\"\n"
    )
}

/// The snapshot's system prompt followed by alternating user and assistant
/// messages.
fn conversation(turns: &[&str]) -> Vec<(String, String)> {
    let roles = ["user", "assistant"].into_iter().cycle();
    let mut messages = vec![("system".to_string(), TERSE.to_string())];
    messages.extend(
        roles
            .zip(turns)
            .map(|(role, text)| (role.to_string(), (*text).to_string())),
    );
    messages
}

/// Every request used the snapshot's model, system prompt, and credential.
fn assert_snapshot(requests: &[RecordedRequest]) {
    let system = ("system".to_string(), TERSE.to_string());
    assert!(
        requests.iter().all(|request| request.model == "fake-model"
            && request.authorization_present
            && request.messages[0] == system),
        "{requests:?}"
    );
}

fn ask(home: &Path, args: &[&str]) -> Output {
    command(home, true).args(args).output().unwrap()
}

fn piped(home: &Path, args: &[&str], payload: &[u8]) -> Output {
    let mut child = command(home, true)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(payload).unwrap();
    child.wait_with_output().unwrap()
}

fn answered(output: &Output, stdout: &[u8]) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(output.stdout, stdout);
    assert!(stderr.starts_with("ask: fake-model · "), "{stderr}");
}

/// Asserts exit status 1 with `stdout`, and returns stderr.
fn failed_with(output: &Output, stdout: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        (output.status.code(), &output.stdout[..]),
        (Some(1), stdout)
    );
    stderr
}

/// Checks each single-value query against the recorded database.
fn assert_rows(home: &Path, expectations: &[(&str, &str)]) {
    let connection = Connection::open(home.join("data/ask.sqlite3")).unwrap();
    for (query, expected) in expectations {
        let actual: Option<String> = connection
            .query_row(&format!("SELECT CAST(({query}) AS TEXT)"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(actual.as_deref(), Some(*expected), "{query}");
    }
}

/// Makes the data directory and database read-only. Returns whether this
/// user can still write there, in which case the proof cannot run.
fn lock_down(data: &Path, database: &Path) -> bool {
    set_mode(database, 0o400);
    set_mode(data, 0o500);
    let probe = data.join("probe");
    let writable = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(probe);
    writable
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn entries(directory: &Path) -> usize {
    fs::read_dir(directory).unwrap().count()
}
