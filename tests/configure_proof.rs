//! End-to-end proof of the configuration commands through the real binary.
//!
//! `ask init` creates a first configuration interactively. `ask configure check`
//! validates a complete candidate document without writing, and
//! `ask configure apply` installs one. Every case here drives the installed
//! binary using redirected input, a PTY, or synchronized OS boundaries. Only
//! queries proving the written configuration is usable reach a fake provider.

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

const CANCELLED: &str = "ask: configuration cancelled; nothing was written\n";

const CANDIDATE: &str = r#"# Comments, key order, and spacing survive `ask configure apply` unchanged.
default_profile = "terse"

[providers.local]
kind         = "openai-compatible"
base_url     = "http://127.0.0.1:1/v1"
api_key_env  = "LOCAL_API_KEY"
timeout_ms   = 41

[profiles.terse]
provider = "local"
model = "fake-model"
system_prompt = "Use terse tables."
"#;

// -- ask init ---------------------------------------------------------------

#[test]
fn init_then_query_answers_through_the_fake_provider() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = fresh_home();
    let answers = format!(
        "local\n{}\nfake-model\nLOCAL_API_KEY\nUse terse tables.\n\ny\n",
        fake.base_url()
    );
    let transcript = succeeded(&interactive(&home, "init", &answers));
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
fn init_alias_i_creates_the_same_file() {
    let home = fresh_home();
    let answers = "local\nhttp://127.0.0.1:1/v1\nfake-model\nLOCAL_API_KEY\n\n\ny\n";
    succeeded(&interactive(&home, "i", answers));
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("default_profile = \"default\""));
    assert!(written.contains("[profiles.default]"));
}

#[test]
fn init_keeps_the_default_system_prompt_when_the_answer_is_empty() {
    let home = fresh_home();
    let answers = "local\nhttp://127.0.0.1:1/v1\nfake-model\nLOCAL_API_KEY\n\n\ny\n";
    assert!(interactive(&home, "init", answers).status.success());
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(!written.contains("system_prompt"), "{written}");
}

#[test]
fn init_has_no_all_default_mode() {
    let home = fresh_home();
    let transcript = failed(&interactive(&home, "init", ""));
    assert!(transcript.ends_with(CANCELLED), "{transcript}");
    assert!(!home.join("config.toml").exists());
}

#[test]
fn init_end_of_input_cancels_with_nothing_written() {
    let home = fresh_home();
    let answers = "local\nhttp://127.0.0.1:1/v1\n";
    let transcript = failed(&interactive(&home, "init", answers));
    assert!(transcript.ends_with(CANCELLED), "{transcript}");
    assert!(!home.join("config.toml").exists());
}

#[test]
fn init_asks_again_after_an_invalid_answer() {
    let home = fresh_home();
    let answers =
        "local\nnot-a-url\nhttp://127.0.0.1:1/v1\nfake-model\n9KEY\nLOCAL_API_KEY\n\n\ny\n";
    let transcript = succeeded(&interactive(&home, "init", answers));
    assert!(transcript.contains(
        "That value must be an http:// or https:// URL with a host, no embedded credentials, and no query or fragment component."
    ));
    assert!(transcript.contains("That value must be an environment variable name"));
    assert!(home.join("config.toml").exists());
}

#[test]
fn init_refuses_an_existing_configuration_and_points_at_apply() {
    let home = fresh_home();
    let path = home.join("config.toml");
    fs::write(&path, "original = true\n").unwrap();
    let transcript = failed(&interactive(&home, "init", ""));
    assert!(
        transcript.starts_with("ask: configuration already exists at '")
            && transcript.ends_with("use 'ask configure apply' to replace it\n"),
        "{transcript}"
    );
    assert_eq!(transcript.lines().count(), 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), "original = true\n");
}

#[cfg(unix)]
#[test]
fn init_failed_disk_write_leaves_no_config_and_allows_retry() {
    let home = fresh_home();
    let answers = format!(
        "local\nhttp://127.0.0.1:1/v1\nfake-model\nLOCAL_API_KEY\n{}\n\ny\n",
        "x".repeat(4096)
    );
    let transcript = failed(&drive(&mut write_limited(&home, &["init"]), &answers));
    assert!(transcript.contains("cannot write"), "{transcript}");
    assert!(!home.join("config.toml").exists());
    assert_eq!(fs::read_dir(&home).unwrap().count(), 1);
    let retry = interactive(&home, "init", &answers);
    assert!(retry.status.success(), "{}", stderr(&retry));
}

// -- ask configure check ----------------------------------------------------

#[test]
fn check_accepts_a_candidate_from_a_file_and_from_standard_input() {
    let home = fresh_home();
    let candidate = home.join("candidate.toml");
    fs::write(&candidate, CANDIDATE).unwrap();
    for (verb, source) in [
        ("configure", candidate.display().to_string()),
        ("c", "-".to_string()),
    ] {
        let transcript = succeeded(&configure(&home, &[verb, "check", &source], CANDIDATE));
        assert!(
            transcript.ends_with("is a valid configuration\n"),
            "{transcript}"
        );
    }
}

#[test]
fn check_reads_standard_input_when_the_argument_is_omitted() {
    let home = fresh_home();
    let transcript = succeeded(&configure(&home, &["configure", "check"], CANDIDATE));
    assert_eq!(transcript, "ask: standard input is a valid configuration\n");
}

#[test]
fn check_writes_nothing_and_never_touches_an_installed_configuration() {
    let home = fresh_home();
    let installed = home.join("config.toml");
    fs::write(&installed, "installed = true\n").unwrap();
    let output = configure(&home, &["configure", "check", "-"], CANDIDATE);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(&installed).unwrap(),
        "installed = true\n"
    );
    let mut names: Vec<_> = fs::read_dir(&home)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec![std::ffi::OsString::from("config.toml")]);
}

#[test]
fn check_rejects_an_unknown_field_without_repeating_the_document() {
    let home = fresh_home();
    let candidate = format!("retention_days = 7\n{CANDIDATE}");
    let output = configure(&home, &["configure", "check", "-"], &candidate);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let message = stderr(&output);
    assert!(
        message.contains("configuration: unknown field at key index "),
        "{message}"
    );
    assert!(!message.contains("Use terse tables."), "{message}");
    assert!(!message.contains("127.0.0.1"), "{message}");
}

#[test]
fn check_validates_every_entry_not_only_the_selected_one() {
    let home = fresh_home();
    let cases = [
        (
            format!(
                "{CANDIDATE}\n[providers.unused]\nkind = \"proprietary\"\nbase_url = \"http://x.test/v1\"\napi_key_env = \"K\"\n"
            ),
            "ask: standard input is not a valid configuration: providers[2].kind has an unsupported kind; the only supported kind is 'openai-compatible'\n",
        ),
        (
            format!("{CANDIDATE}\n[profiles.unused]\nprovider = \"absent\"\nmodel = \"m\"\n"),
            "ask: standard input is not a valid configuration: profiles[2].provider references an unknown provider\n",
        ),
        (
            CANDIDATE.replace("timeout_ms   = 41", "timeout_ms   = 0"),
            "ask: standard input is not a valid configuration: providers[1].timeout_ms must be greater than zero\n",
        ),
        (
            CANDIDATE.replace(
                "default_profile = \"terse\"",
                "default_profile = \"absent\"",
            ),
            "ask: standard input is not a valid configuration: default_profile references an unknown profile\n",
        ),
    ];
    for (candidate, expected) in cases {
        let transcript = failed(&configure(&home, &["configure", "check", "-"], &candidate));
        assert_eq!(transcript, expected);
    }
}

#[test]
fn check_reports_a_missing_file() {
    let home = fresh_home();
    let absent = home.join("absent.toml");
    let arguments = ["configure", "check", &absent.display().to_string()];
    let transcript = failed(&configure(&home, &arguments, ""));
    assert!(transcript.starts_with("ask: cannot read '"), "{transcript}");
}

// -- ask configure apply ----------------------------------------------------

#[test]
fn apply_creates_a_configuration_with_exact_bytes_then_answers_a_query() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = fresh_home();
    let candidate = CANDIDATE
        .replace("http://127.0.0.1:1/v1", &fake.base_url())
        .replace("timeout_ms   = 41", "timeout_ms   = 30000");
    let transcript = succeeded(&configure(&home, &["configure", "apply", "-"], &candidate));
    assert!(transcript.starts_with("ask: created '"), "{transcript}");
    let installed = home.join("config.toml");
    assert_eq!(fs::read(&installed).unwrap(), candidate.as_bytes());

    let query = command(&home, true).arg("hello").output().unwrap();
    assert!(query.status.success(), "{}", stderr(&query));
    assert_eq!(query.stdout, b"**4**\n");
    let request = fake.recorded().unwrap();
    assert_eq!(
        request.messages[0],
        ("system".to_string(), "Use terse tables.".to_string())
    );
}

#[test]
fn apply_replaces_an_existing_configuration_from_a_file() {
    let home = fresh_home();
    let installed = home.join("config.toml");
    fs::write(&installed, "default_profile = \"old\"\n").unwrap();
    let candidate = home.join("candidate.toml");
    fs::write(&candidate, CANDIDATE).unwrap();
    let arguments = ["c", "apply", &candidate.display().to_string()];
    let transcript = succeeded(&configure(&home, &arguments, ""));
    assert!(transcript.starts_with("ask: replaced '"), "{transcript}");
    assert_eq!(fs::read(&installed).unwrap(), CANDIDATE.as_bytes());
}

#[test]
fn apply_leaves_the_installed_configuration_alone_when_the_candidate_is_invalid() {
    let home = fresh_home();
    let installed = home.join("config.toml");
    fs::write(&installed, CANDIDATE).unwrap();
    let candidate = CANDIDATE.replace("model =", "modle =");
    let transcript = failed(&configure(&home, &["configure", "apply", "-"], &candidate));
    assert!(
        transcript.contains("is not a valid configuration"),
        "{transcript}"
    );
    assert_eq!(fs::read(&installed).unwrap(), CANDIDATE.as_bytes());
    assert_eq!(fs::read_dir(&home).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn apply_failed_disk_write_leaves_the_previous_configuration_intact() {
    let home = fresh_home();
    let installed = home.join("config.toml");
    fs::write(&installed, CANDIDATE).unwrap();
    let candidate = CANDIDATE.replace(
        "system_prompt = \"Use terse tables.\"",
        &format!("system_prompt = \"{}\"", "x".repeat(4096)),
    );
    let mut limited = write_limited(&home, &["configure", "apply", "-"]);
    let transcript = failed(&drive(&mut limited, &candidate));
    assert!(transcript.contains("cannot write"), "{transcript}");
    assert_eq!(fs::read(&installed).unwrap(), CANDIDATE.as_bytes());
    assert_eq!(fs::read_dir(&home).unwrap().count(), 2);
}

// -- usage ------------------------------------------------------------------

#[test]
fn usage_errors_exit_two() {
    let home = fresh_home();
    for arguments in [
        vec!["init", "now"],
        vec!["i", "now"],
        vec!["configure"],
        vec!["c"],
        vec!["configure", "edit"],
        vec!["configure", "check", "one", "two"],
    ] {
        let transcript = misused(&configure(&home, &arguments, ""));
        assert!(transcript.starts_with("ask: usage: "), "{arguments:?}");
    }
    assert!(!home.join("config.toml").exists());
}

// -- helpers ----------------------------------------------------------------

/// Asserts the run succeeded with an empty stdout and returns its stderr.
fn succeeded(output: &Output) -> String {
    assert!(output.status.success(), "{}", stderr(output));
    assert!(output.stdout.is_empty());
    stderr(output)
}

/// Asserts the run failed with `status` and an empty stdout, returning its stderr.
fn refused(output: &Output, status: i32) -> String {
    assert_eq!(output.status.code(), Some(status), "{}", stderr(output));
    assert!(output.stdout.is_empty());
    stderr(output)
}

fn failed(output: &Output) -> String {
    refused(output, 1)
}

fn misused(output: &Output) -> String {
    refused(output, 2)
}

fn interactive(home: &Path, verb: &str, answers: &str) -> Output {
    drive(command(home, true).arg(verb), answers)
}

fn configure(home: &Path, arguments: &[&str], input: &str) -> Output {
    drive(command(home, false).args(arguments), input)
}

/// Runs the binary under a one-block file-size limit so any write beyond the
/// first block fails. `SIGXFSZ` is ignored so the child reports the error itself.
#[cfg(unix)]
fn write_limited(home: &Path, arguments: &[&str]) -> Command {
    let mut limited = Command::new("sh");
    limited
        .args([
            "-c",
            "trap '' XFSZ; ulimit -f 1; exec \"$@\"",
            "write-limit",
            env!("CARGO_BIN_EXE_ask"),
        ])
        .args(arguments)
        .env("ASK_HOME", home)
        .env_remove("LOCAL_API_KEY")
        // The disk limit would also truncate this child's coverage profile.
        .env("LLVM_PROFILE_FILE", "/dev/null");
    limited
}

fn drive(command: &mut Command, input: &str) -> Output {
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
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn candidate_text_never_appears_in_validation_diagnostics() {
    let home = fresh_home();
    let sentinel = "SECRET_SENTINEL";
    let cases = [
        format!("{sentinel} = 7\n{CANDIDATE}"),
        CANDIDATE.replace("model =", &format!("{sentinel} =")),
        CANDIDATE.replace("[providers.local]", &format!("[providers.{sentinel}]")),
        CANDIDATE.replace("[profiles.terse]", &format!("[profiles.{sentinel}]")),
        CANDIDATE.replace(
            "provider = \"local\"",
            &format!("provider = \"{sentinel}\""),
        ),
        CANDIDATE.replace(
            "default_profile = \"terse\"",
            &format!("default_profile = \"{sentinel}\""),
        ),
        CANDIDATE.replace(
            "timeout_ms   = 41",
            &format!("timeout_ms = '{sentinel}, expected {sentinel}'"),
        ),
        format!("{sentinel} = 1\n{sentinel} = 2\n"),
        format!("[\"{sentinel}\"\n"),
    ];
    for candidate in cases {
        for action in ["check", "apply"] {
            let message = failed(&configure(&home, &["configure", action, "-"], &candidate));
            assert!(!message.contains(sentinel), "{message}");
        }
    }
}

#[test]
fn explicit_empty_system_prompt_remains_supported() {
    let home = fresh_home();
    let candidate = CANDIDATE.replace("Use terse tables.", "");
    succeeded(&configure(&home, &["configure", "apply", "-"], &candidate));
    assert_eq!(
        fs::read_to_string(home.join("config.toml")).unwrap(),
        candidate
    );
}

#[cfg(unix)]
#[test]
fn process_proofs_refuse_disabled_assertions() {
    let home = fresh_home();
    fs::write(home.join("candidate.toml"), CANDIDATE).unwrap();
    for script in ["configuration_process.py", "publication_failure.py"] {
        let output = Command::new("python3")
            .arg(format!("tests/support/{script}"))
            .args(["/bin/true"])
            .arg(&home)
            .arg("terminal")
            .env("PYTHONOPTIMIZE", "1")
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{script} accepted disabled assertions"
        );
        assert!(stderr(&output).contains("proofs require Python assertions"));
    }
}

#[cfg(unix)]
#[test]
fn synchronized_configuration_process_proofs() {
    for scenario in [
        "terminal",
        "content",
        "identity",
        "appeared",
        "exclusion",
        "init_interaction",
        "utf8",
    ] {
        let home = fresh_home();
        fs::write(home.join("candidate.toml"), CANDIDATE).unwrap();
        let output = Command::new("python3")
            .arg("tests/support/configuration_process.py")
            .arg(env!("CARGO_BIN_EXE_ask"))
            .arg(&home)
            .arg(scenario)
            .output()
            .unwrap();
        assert!(output.status.success(), "{scenario}: {}", stderr(&output));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn final_publication_failure_preserves_creation_and_replacement_destinations() {
    for mode in ["create", "replace", "appeared"] {
        let home = fresh_home();
        fs::write(home.join("candidate.toml"), CANDIDATE).unwrap();
        let output = Command::new("python3")
            .arg("tests/support/publication_failure.py")
            .arg(env!("CARGO_BIN_EXE_ask"))
            .arg(&home)
            .arg(mode)
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", stderr(&output));
    }
}

#[test]
fn installed_configuration_errors_use_the_same_safe_validator() {
    let home = fresh_home();
    let missing = command(&home, false).arg("hello").output().unwrap();
    assert!(failed(&missing).contains("cannot read"));
    fs::write(home.join("config.toml"), "SECRET_SENTINEL = 1\n").unwrap();
    let invalid = command(&home, false).arg("hello").output().unwrap();
    let message = failed(&invalid);
    assert!(message.contains("configuration: unknown field at key index"));
    assert!(!message.contains("SECRET_SENTINEL"));
}

#[test]
fn platform_configuration_path_is_used_without_ask_home() {
    let home = fresh_home();
    let output = command(&home, false)
        .arg("hello")
        .env_remove("ASK_HOME")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .output()
        .unwrap();
    let message = failed(&output);
    assert!(message.contains("cannot read"), "{message}");
    assert!(message.contains(&home.display().to_string()), "{message}");
}
