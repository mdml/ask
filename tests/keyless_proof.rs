//! End-to-end proof of a target with no credential variable: the fixed
//! placeholder is the only credential on the wire, no environment value
//! reaches the request, and replies and `ask doctor` work without a key.

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use support::{
    CREDENTIAL, command,
    fake_provider::{FakeProvider, RecordedRequest, Scenario},
    fresh_home,
};

const PLACEHOLDER: &str = "Bearer no-key";
/// A local reasoning model's stream: thinking arrives in `reasoning` and
/// `reasoning_content` beside `content`, with an inline `<think>` block too.
const THINKING_STREAM: &str = concat!(
    "data: {\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning\":\"THINK-ONE\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"THINK-TWO\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning\":\"THINK-THREE\",\"content\":\"**4**\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"total_tokens\":15}}\n\n",
    "data: [DONE]\n\n",
);
const KEYLESS_CONFIG: &str = "default_profile = \"default\"\n\n[providers.local]\nkind = \"openai-compatible\"\nbase_url = \"{URL}\"\n\n[profiles.default]\nprovider = \"local\"\nmodel = \"fake-model\"\n";

#[test]
fn a_query_sends_only_the_placeholder_even_with_credential_variables_set() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = keyless_home(&fake);
    let output = ask(&home, &["question"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, b"**4**\n");
    assert_placeholder_only(&fake.recorded().unwrap());
}

#[test]
fn a_reply_after_the_configuration_changes_still_sends_only_the_placeholder() {
    let fake = FakeProvider::sequence(vec![Scenario::Stream]);
    let home = keyless_home(&fake);
    assert!(ask(&home, &["question"]).status.success());
    let keyed = KEYLESS_CONFIG.replace(
        "\n[profiles",
        "api_key_env = \"LOCAL_API_KEY\"\n\n[profiles",
    );
    fs::write(
        home.join("config.toml"),
        keyed.replace("{URL}", &fake.base_url()),
    )
    .unwrap();
    let reply = ask(&home, &["reply", "again"]);
    assert!(reply.status.success(), "{}", stderr(&reply));
    let requests = fake.requests(2);
    assert_eq!(requests.len(), 2);
    requests.iter().for_each(assert_placeholder_only);
}

#[test]
fn a_reply_works_with_the_configuration_removed() {
    let fake = FakeProvider::sequence(vec![Scenario::Stream]);
    let home = keyless_home(&fake);
    assert!(ask(&home, &["question"]).status.success());
    fs::remove_file(home.join("config.toml")).unwrap();
    let reply = ask(&home, &["reply", "again"]);
    assert!(reply.status.success(), "{}", stderr(&reply));
    fake.requests(2).iter().for_each(assert_placeholder_only);
}

#[test]
fn the_placeholder_text_is_not_redacted_from_diagnostics() {
    let fake = FakeProvider::start(Scenario::Status(500, "no-key rejected"));
    let home = keyless_home(&fake);
    let output = ask(&home, &["question"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("no-key"), "{}", stderr(&output));
    assert!(!stderr(&output).contains("[redacted]"));
}

#[test]
fn doctor_reports_not_required_and_is_not_an_environment_problem() {
    let fake = FakeProvider::start(Scenario::Answer("ok"));
    let home = keyless_home(&fake);
    let output = bare(&home, &["doctor"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("credential: not required"), "{stdout}");
}

#[test]
fn live_doctor_checks_a_keyless_target_with_the_placeholder() {
    let fake = FakeProvider::start(Scenario::Answer("ok"));
    let home = keyless_home(&fake);
    let output = bare(&home, &["doctor", "--live"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains("live: ok"));
    assert_placeholder_only(&fake.recorded().unwrap());
}

#[test]
fn reasoning_fields_reach_neither_stdout_nor_the_recorded_turn() {
    let fake = FakeProvider::sequence(vec![Scenario::Sse(THINKING_STREAM), Scenario::Stream]);
    let home = keyless_home(&fake);
    let output = ask(&home, &["question"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, b"**4**\n");
    assert!(!stderr(&output).contains("THINK"), "{}", stderr(&output));
    assert!(ask(&home, &["reply", "again"]).status.success());
    let replay = fake.requests(2)[1].body.to_string();
    assert!(replay.contains("**4**"), "{replay}");
    assert!(!replay.contains("THINK"), "{replay}");
}

fn assert_placeholder_only(request: &RecordedRequest) {
    assert_eq!(request.header("authorization"), Some(PLACEHOLDER));
    assert!(!request.authorization_is_fixture);
    let wire = format!("{:?}", request.headers) + &request.body.to_string();
    assert!(!wire.contains(CREDENTIAL), "{wire}");
    assert!(!wire.contains("OPENAI-ENV-SECRET"), "{wire}");
}

fn keyless_home(fake: &FakeProvider) -> PathBuf {
    let home = fresh_home();
    let config = KEYLESS_CONFIG.replace("{URL}", &fake.base_url());
    fs::write(home.join("config.toml"), config).unwrap();
    home
}

/// Runs `ask` with unrelated credential variables set.
fn ask(home: &Path, args: &[&str]) -> Output {
    command(home, true)
        .env("OPENAI_API_KEY", "OPENAI-ENV-SECRET")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// Runs `ask` with no credential variable set.
fn bare(home: &Path, args: &[&str]) -> Output {
    command(home, false)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
