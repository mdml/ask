mod support;

use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use support::{
    CREDENTIAL, command,
    fake_provider::{FakeProvider, Scenario},
    fresh_home,
};

#[test]
fn configure_then_query_answers_through_the_fake_provider() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = fresh_home();
    let answers = format!(
        "local\n{}\nfake-model\nLOCAL_API_KEY\nUse terse tables.\n\ny\n",
        fake.base_url()
    );
    let output = configure(&home, "configure", &answers);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
    let transcript = stderr(&output);
    assert!(transcript.contains(ask::DEFAULT_SYSTEM_PROMPT));
    assert!(transcript.contains("LOCAL_API_KEY"));
    assert!(!transcript.contains(CREDENTIAL));
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("api_key_env = \"LOCAL_API_KEY\""));
    assert!(!written.contains(CREDENTIAL));

    let query = command(&home, true)
        .args(["what", "is", "2+2"])
        .output()
        .unwrap();
    assert!(query.status.success(), "{}", stderr(&query));
    assert_eq!(query.stdout, b"**4**\n");
    let request = fake.recorded().unwrap();
    assert_eq!(request.model, "fake-model");
    assert_eq!(
        request.messages[0],
        ("system".to_string(), "Use terse tables.".to_string())
    );
}

#[test]
fn alias_c_creates_the_same_file() {
    let home = fresh_home();
    let answers = "local\nhttp://127.0.0.1:1/v1\nfake-model\nLOCAL_API_KEY\n\n\ny\n";
    let output = configure(&home, "c", answers);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("default_profile = \"default\""));
    assert!(written.contains("[profiles.default]"));
}

#[test]
fn end_of_input_cancels_with_nothing_written() {
    let home = fresh_home();
    let output = configure(&home, "configure", "local\nhttp://127.0.0.1:1/v1\n");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        stderr(&output).ends_with("ask: configuration cancelled; nothing was written\n"),
        "{}",
        stderr(&output)
    );
    assert!(!home.join("config.toml").exists());
}

#[test]
fn invalid_answers_are_asked_again_on_stderr() {
    let home = fresh_home();
    let answers =
        "local\nnot-a-url\nhttp://127.0.0.1:1/v1\nfake-model\n9KEY\nLOCAL_API_KEY\n\n\ny\n";
    let output = configure(&home, "configure", answers);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
    let transcript = stderr(&output);
    assert!(
        transcript
            .contains("Enter an http:// or https:// URL with a host and no embedded credentials.")
    );
    assert!(transcript.contains("Enter an environment variable name"));
    assert!(home.join("config.toml").exists());
}

#[test]
fn existing_configuration_is_refused_and_left_unchanged() {
    let home = fresh_home();
    let path = home.join("config.toml");
    fs::write(&path, "original = true\n").unwrap();
    let output = configure(&home, "configure", "local\n");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let transcript = stderr(&output);
    assert!(
        transcript.starts_with("ask: configuration already exists at '"),
        "{transcript}"
    );
    assert_eq!(transcript.lines().count(), 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), "original = true\n");
}

#[test]
fn extra_arguments_are_a_usage_error() {
    let home = fresh_home();
    let output = configure(&home, "configure", "");
    assert_eq!(output.status.code(), Some(1));
    let output = command(&home, false)
        .args(["configure", "now"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).starts_with("ask: usage: "));
}

#[cfg(unix)]
#[test]
fn failed_disk_write_leaves_no_config_and_allows_retry() {
    let home = fresh_home();
    let answers = format!(
        "local\nhttp://127.0.0.1:1/v1\nfake-model\nLOCAL_API_KEY\n{}\n\ny\n",
        "x".repeat(4096)
    );
    let mut limited = Command::new("sh");
    limited
        .args([
            "-c",
            "trap '' XFSZ; ulimit -f 1; exec \"$1\" configure",
            "write-limit",
        ])
        .arg(env!("CARGO_BIN_EXE_ask"))
        .env("ASK_HOME", &home)
        // The disk limit would also truncate this child's coverage profile.
        .env("LLVM_PROFILE_FILE", "/dev/null");
    let output = drive(&mut limited, &answers);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("cannot write"));
    assert!(output.stdout.is_empty());
    assert!(!home.join("config.toml").exists());
    assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
    let retry = configure(&home, "configure", &answers);
    assert!(retry.status.success(), "{}", stderr(&retry));
}

fn configure(home: &Path, verb: &str, answers: &str) -> Output {
    drive(command(home, true).arg(verb), answers)
}

fn drive(command: &mut Command, answers: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(answers.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
