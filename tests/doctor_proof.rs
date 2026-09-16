//! End-to-end proof of `ask doctor` offline diagnostics and live checks.

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use support::{
    CREDENTIAL, command,
    fake_provider::{FakeProvider, Scenario},
    fresh_home,
};

#[test]
fn offline_doctor_reports_valid_configuration_and_paths_without_creating_database() {
    let fake = FakeProvider::start(Scenario::Answer("ok"));
    let home = configured(&fake, false);
    let output = ask(&home, &["doctor"], true);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(output.stderr.is_empty(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("configuration: valid"));
    assert!(stdout.contains("ask_home:"));
    assert!(stdout.contains("config.toml (present)"));
    assert!(stdout.contains("data/ask.sqlite3 (absent)"));
    assert!(stdout.contains("cache (absent)"));
    assert!(stdout.contains("credential LOCAL_API_KEY: present"));
    assert!(!stdout.contains(CREDENTIAL));
    assert!(!home.join("data/ask.sqlite3").exists());
}

#[test]
fn offline_doctor_fails_for_invalid_configuration() {
    let home = fresh_home();
    fs::write(home.join("config.toml"), "default_profile = \"missing\"\n").unwrap();
    let output = ask(&home, &["doctor"], false);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("configuration: invalid"));
    assert!(stderr(&output).contains("doctor found configuration problems"));
}

#[test]
fn offline_doctor_reports_missing_credential_as_environment_failure() {
    let fake = FakeProvider::start(Scenario::Answer("ok"));
    let home = configured(&fake, false);
    let output = ask(&home, &["doctor"], false);
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("credential LOCAL_API_KEY: missing"));
    assert!(stderr(&output).contains("doctor found environment problems"));
}

#[test]
fn live_doctor_checks_the_default_target_against_the_fake_provider() {
    let fake = FakeProvider::start(Scenario::Answer("ok"));
    let home = configured(&fake, false);
    let output = ask(&home, &["doctor", "--live"], true);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("may incur cost"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("live: ok"));
    let request = fake.recorded().unwrap();
    assert_eq!(request.model, "fake-model");
    assert_eq!(request.messages.last().unwrap().1, ask::doctor::LIVE_PROMPT);
    assert_eq!(
        request.messages.first().unwrap().1,
        ask::doctor::LIVE_SYSTEM_PROMPT
    );
    assert_eq!(request.body["max_tokens"].as_u64(), Some(128));
}

#[test]
fn live_all_checks_every_distinct_provider_target() {
    let fake = FakeProvider::sequence(vec![Scenario::Answer("ok"), Scenario::Answer("ok")]);
    let home = multi_profile(&fake);
    let output = ask(&home, &["doctor", "--live", "--all"], true);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fake.requests(2).len(), 2);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("model-a"));
    assert!(stdout.contains("model-b"));
}

#[test]
fn offline_doctor_reports_limited_database_as_environment_failure() {
    let fake = FakeProvider::start(Scenario::Answer("ok"));
    let home = configured(&fake, false);
    fs::create_dir_all(home.join("data")).unwrap();
    let db = home.join("data/ask.sqlite3");
    fs::write(&db, b"not a sqlite database").unwrap();
    let output = ask(&home, &["doctor"], true);
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("database:"));
    assert!(stderr(&output).contains("doctor found environment problems"));
    assert_eq!(fs::read(&db).unwrap(), b"not a sqlite database");
}

#[test]
fn doctor_alias_d_matches_doctor() {
    let fake = FakeProvider::start(Scenario::Answer("ok"));
    let home = configured(&fake, false);
    for name in ["doctor", "d"] {
        let output = ask(&home, &[name], true);
        assert!(output.status.success(), "{name}: {}", stderr(&output));
    }
}

fn configured(fake: &FakeProvider, _with_credential: bool) -> PathBuf {
    configured_home(fake, "default", &[("default", "fake-model")])
}

fn multi_profile(fake: &FakeProvider) -> PathBuf {
    configured_home(fake, "a", &[("a", "model-a"), ("b", "model-b")])
}

fn configured_home(
    fake: &FakeProvider,
    default_profile: &str,
    profiles: &[(&str, &str)],
) -> PathBuf {
    let home = fresh_home();
    let mut config = format!(
        "default_profile = \"{default_profile}\"\n\n[providers.local]\nkind = \"openai-compatible\"\nbase_url = \"{}\"\napi_key_env = \"LOCAL_API_KEY\"\n\n",
        fake.base_url()
    );
    for (profile, model) in profiles {
        config.push_str(&format!(
            "[profiles.{profile}]\nprovider = \"local\"\nmodel = \"{model}\"\n\n"
        ));
    }
    fs::write(home.join("config.toml"), config).unwrap();
    home
}

fn ask(home: &Path, args: &[&str], with_credential: bool) -> Output {
    command(home, with_credential)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
