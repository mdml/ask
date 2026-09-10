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
fn blank_input_is_rejected_after_configuration_and_credential_checks() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured_home(&fake.base_url(), None, None);
    for (args, payload) in [
        (&[][..], b"".as_slice()),
        (&[""][..], b""),
        (&[" ", "\t"][..], b"payload"),
    ] {
        let output = piped(&home, args, payload);
        assert!(fake.recorded().is_none());
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "ask: query must contain non-whitespace text\n"
        );
    }
    let unconfigured = fresh_home();
    let output = piped(&unconfigured, &[], b"");
    assert_eq!(output.status.code(), Some(1));
    assert!(fake.recorded().is_none());
}

#[test]
fn early_pipe_closure_is_quiet_and_successful() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured_home(&fake.base_url(), None, None);
    let mut command = command(&home, true);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child.stdin.take().unwrap().write_all(b"question").unwrap();
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

#[test]
fn piped_input_supplies_the_prompt_for_all_query_forms() {
    for args in [&[][..], &["new"][..], &["n"][..]] {
        let fake = FakeProvider::start(Scenario::Stream);
        let home = configured_home(&fake.base_url(), None, None);
        let output = piped(&home, args, b"  first line\nsecond line\n");
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            fake.recorded().unwrap().messages[1].1,
            "  first line\nsecond line\n"
        );
        assert_eq!(output.stdout, b"**4**\n");
        assert_statistics(&output.stderr, "12 in / 3 out");
    }
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

#[test]
fn piped_payload_composes_with_instruction_and_preserves_utf8() {
    for args in [
        &["  describe", "this  "][..],
        &["new", "  describe", "this  "][..],
        &["n", "  describe", "this  "][..],
    ] {
        let fake = FakeProvider::start(Scenario::Stream);
        let home = configured_home(&fake.base_url(), None, None);
        let payload = "  café 日本語 🦀\r\n\n";
        let output = piped(&home, args, payload.as_bytes());
        assert!(output.status.success());
        assert_eq!(
            fake.recorded().unwrap().messages[1].1.as_bytes(),
            format!("describe this\n\n{payload}").as_bytes()
        );
    }
}

#[test]
fn empty_redirected_input_is_a_usage_error_without_a_request() {
    for payload in [b"".as_slice(), b" \t\r\n"] {
        let fake = FakeProvider::start(Scenario::Stream);
        let home = configured_home(&fake.base_url(), None, None);
        let output = piped(&home, &[], payload);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
        assert_no_request(&fake, &output);
    }
}

#[test]
fn invalid_utf8_stdin_fails_without_a_request() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured_home(&fake.base_url(), None, None);
    let output = piped(&home, &["describe"], b"\xffPRIVATE_INPUT");
    assert_failure(&output, "cannot read standard input");
    assert!(output.stderr.starts_with(b"ask: "));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_INPUT"));
    assert_no_request(&fake, &output);
}

#[test]
fn stdin_read_failure_is_one_line_without_a_request() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = configured_home(&fake.base_url(), None, None);
    let output = command(&home, true)
        .stdin(fs::File::open(&home).unwrap())
        .output()
        .unwrap();
    assert_failure(&output, "cannot read standard input");
    assert!(output.stderr.starts_with(b"ask: "));
    assert!(output.stdout.is_empty());
    assert!(fake.recorded().is_none());
}

#[test]
fn piped_input_preserves_provider_and_streaming_failures() {
    for (scenario, answer) in [
        (Scenario::Unauthorized, b"".as_slice()),
        (Scenario::PartialFailure, b"partial\n"),
    ] {
        let fake = FakeProvider::start(scenario);
        let home = configured_home(&fake.base_url(), None, None);
        let output = piped(&home, &[], b"question from stdin");
        assert_failure(&output, "provider request failed");
        assert_eq!(output.stdout, answer);
        assert_eq!(
            fake.recorded().unwrap().messages[1].1,
            "question from stdin"
        );
    }
}

#[test]
fn terminal_words_never_wait_for_input() {
    for args in [
        &["one", "question"][..],
        &["new", "one", "question"][..],
        &["n", "one", "question"][..],
    ] {
        let fake = FakeProvider::start(Scenario::Stream);
        let home = configured_home(&fake.base_url(), None, None);
        terminal(&home, "words", args);
        assert_eq!(fake.recorded().unwrap().messages[1].1, "one question");
    }
}

#[test]
fn terminal_multiline_submits_on_eof() {
    for args in [&[][..], &["new"][..], &["n"][..]] {
        let fake = FakeProvider::start(Scenario::Stream);
        let home = configured_home(&fake.base_url(), None, None);
        terminal(&home, "multiline", args);
        assert_eq!(
            fake.recorded().unwrap().messages[1].1,
            "first line\nsecond line\n"
        );
    }
}

#[test]
fn terminal_empty_submission_sends_no_request() {
    for scenario in ["empty", "whitespace"] {
        let fake = FakeProvider::start(Scenario::Stream);
        let home = configured_home(&fake.base_url(), None, None);
        terminal(&home, scenario, &[]);
        assert!(fake.recorded().is_none());
    }
}

fn terminal(home: &Path, scenario: &str, args: &[&str]) {
    let output = std::process::Command::new("python3")
        .arg("tests/support/query_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(home)
        .arg(scenario)
        .args(args)
        .env("LOCAL_API_KEY", CREDENTIAL)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_no_request(fake: &FakeProvider, output: &Output) {
    assert!(output.stdout.is_empty());
    assert!(fake.recorded().is_none());
}
