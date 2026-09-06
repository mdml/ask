mod support;

use std::{
    fs,
    io::Write,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use support::{
    CREDENTIAL, command,
    fake_provider::{FakeProvider, Scenario},
    fresh_home,
};

#[test]
fn happy_path_separates_answer_and_statistics() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured_home(&fake.base_url(), None, None);
    let output = ask(&home, &["what", "is", "2+2"], true);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"**4**\n");
    assert_statistics(&output.stderr, "12 in / 3 out");
    assert!(!output.stdout.contains(&0x1b));
    let request = fake.recorded().unwrap();
    assert_eq!(request.path, "/v1/chat/completions");
    assert!(request.authorization_present);
    assert_eq!(request.model, "fake-model");
    assert_eq!(
        request.messages,
        vec![
            ("system".to_string(), ask::DEFAULT_SYSTEM_PROMPT.to_string()),
            ("user".to_string(), "what is 2+2".to_string())
        ]
    );
}

#[test]
fn profile_replaces_the_default_system_prompt() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured_home(&fake.base_url(), Some("Use terse tables."), None);
    let output = ask(&home, &["question"], true);
    assert!(output.status.success());
    let request = fake.recorded().unwrap();
    assert_eq!(
        request.messages[0],
        ("system".into(), "Use terse tables.".into())
    );
}

#[test]
fn all_three_command_forms_join_prompt_words() {
    let forms: [&[&str]; 3] = [
        &["one", "question"],
        &["new", "one", "question"],
        &["n", "one", "question"],
    ];
    for form in forms {
        let fake = FakeProvider::start(Scenario::Stream);
        let home = configured_home(&fake.base_url(), None, None);
        let output = ask(&home, form, true);
        assert!(output.status.success());
        assert_eq!(fake.recorded().unwrap().messages[1].1, "one question");
    }
}

#[test]
fn missing_usage_is_shown_as_unknown() {
    let fake = FakeProvider::start(Scenario::StreamWithoutUsage);
    let home = configured_home(&fake.base_url(), None, None);
    let output = ask(&home, &["question"], true);
    assert!(output.status.success());
    assert_statistics(&output.stderr, "? in / ? out");
}

#[test]
fn authentication_failure_is_one_line_and_safe() {
    assert_http_failure(Scenario::Unauthorized, 401);
}

#[test]
fn rate_limit_failure_is_one_line_and_safe() {
    assert_http_failure(Scenario::RateLimited, 429);
}

#[test]
fn malformed_stream_fails_without_answer_text() {
    let fake = FakeProvider::start(Scenario::Malformed);
    let home = configured_home(&fake.base_url(), None, None);
    let output = ask(&home, &["question"], true);
    assert_failure(&output, "provider request failed");
    assert!(output.stdout.is_empty());
}

#[test]
fn stalled_response_obeys_configured_timeout() {
    let fake = FakeProvider::start(Scenario::Stall);
    let home = configured_home(&fake.base_url(), None, Some(250));
    let output = ask(&home, &["question"], true);
    assert_failure(&output, "timed out after 250 ms");
    assert!(output.stdout.is_empty());
}

#[test]
fn connection_refusal_is_a_provider_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let home = configured_home(&format!("http://{address}/v1"), None, None);
    let output = ask(&home, &["question"], true);
    assert_failure(&output, "provider request failed");
    assert!(output.stdout.is_empty());
}

#[test]
fn streaming_failure_preserves_partial_answer() {
    let fake = FakeProvider::start(Scenario::PartialFailure);
    let home = configured_home(&fake.base_url(), None, None);
    let output = ask(&home, &["question"], true);
    assert_failure(&output, "provider request failed");
    assert_eq!(output.stdout, b"partial\n");
}

#[test]
fn missing_credential_names_variable_and_sends_no_request() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured_home(&fake.base_url(), None, None);
    let output = ask(&home, &["question"], false);
    assert_failure(
        &output,
        "credential environment variable 'LOCAL_API_KEY' is not set",
    );
    assert!(output.stdout.is_empty());
    assert!(fake.recorded().is_none());
}

#[test]
fn no_prompt_is_a_usage_error_with_status_two() {
    let home = fresh_home();
    let output = ask(&home, &[], false);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"ask: usage: ask [new|n] <prompt words...> | ask [configure|c]\n"
    );
}

#[test]
fn early_pipe_closure_is_quiet_and_successful() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured_home(&fake.base_url(), None, None);
    let mut command = command(&home, true);
    let mut child = command
        .arg("question")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}

#[test]
fn fake_provider_tolerates_a_truncated_request() {
    let fake = FakeProvider::start(Scenario::Stall);
    let mut client = TcpStream::connect(fake.address()).unwrap();
    client
        .write_all(b"POST /v1/chat/completions HTTP/1.1\r\nContent-Length: 100\r\n\r\n{\"model")
        .unwrap();
    drop(client);
    assert!(fake.recorded().is_none());
    drop(fake);
}

fn assert_http_failure(scenario: Scenario, status: u16) {
    let fake = FakeProvider::start(scenario);
    let home = configured_home(&fake.base_url(), None, None);
    let output = ask(&home, &["question"], true);
    assert_failure(&output, &status.to_string());
    assert!(!String::from_utf8_lossy(&output.stderr).contains(CREDENTIAL));
}

fn assert_failure(output: &Output, expected: &str) {
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(expected), "stderr was: {stderr}");
    assert_eq!(stderr.lines().count(), 1, "stderr was: {stderr}");
}

fn assert_statistics(stderr: &[u8], usage: &str) {
    let stderr = String::from_utf8_lossy(stderr);
    assert!(stderr.starts_with("ask: fake-model · "));
    assert!(stderr.contains("s wall · "));
    assert!(stderr.contains("s api · "));
    assert!(stderr.contains("s to first token · "));
    assert!(stderr.contains(usage));
    assert_eq!(stderr.lines().count(), 1);
}

fn ask(home: &Path, args: &[&str], with_credential: bool) -> Output {
    command(home, with_credential).args(args).output().unwrap()
}

fn configured_home(base_url: &str, system_prompt: Option<&str>, timeout: Option<u64>) -> PathBuf {
    let home = fresh_home();
    let timeout = timeout.map_or_else(String::new, |value| format!("timeout_ms = {value}\n"));
    let system = system_prompt.map_or_else(String::new, |value| {
        format!("system_prompt = \"{value}\"\n")
    });
    let config = format!(
        "default_profile = \"default\"\n\n[providers.local]\nkind = \"openai-compatible\"\nbase_url = \"{base_url}\"\napi_key_env = \"LOCAL_API_KEY\"\n{timeout}\n[profiles.default]\nprovider = \"local\"\nmodel = \"fake-model\"\n{system}"
    );
    fs::write(home.join("config.toml"), config).unwrap();
    home
}
