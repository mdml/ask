use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::config::Target;

static CASE_ID: AtomicUsize = AtomicUsize::new(0);

const COMPLETE: &str = "5\nlocal\nhttp://127.0.0.1:1/v1\nLOCAL_API_KEY\nfake-model\n\n\ny\n";

fn fresh_path() -> PathBuf {
    let id = CASE_ID.fetch_add(1, Ordering::SeqCst);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/init-unit-tests")
        .join(format!("{}-{id}", std::process::id()));
    dir.join("config.toml")
}

fn drive(path: &Path, answers: &str) -> (Result<(), InitError>, String) {
    let mut input = Cursor::new(answers.as_bytes().to_vec());
    let mut output = Vec::new();
    let result = run(path, &mut input, &mut output, false);
    (result, String::from_utf8(output).unwrap())
}

fn assert_local_openai_compatible_target(target: &Target) {
    assert_eq!(
        (
            target.base_url.as_str(),
            target.model.as_str(),
            target.api_key_env.as_str(),
            target.kind.as_str(),
            target.system_prompt.as_str()
        ),
        (
            "http://127.0.0.1:1/v1",
            "fake-model",
            "LOCAL_API_KEY",
            "openai-compatible",
            crate::DEFAULT_SYSTEM_PROMPT
        )
    );
}

fn assert_complete_dialogue_transcript(transcript: &str) {
    for expected in [
        crate::DEFAULT_SYSTEM_PROMPT,
        "read -rs LOCAL_API_KEY",
        "docs/guides/credentials.md",
        "Write this configuration? [y/N]: Wrote '",
    ] {
        assert!(
            transcript.contains(expected),
            "missing dialogue text: {expected}"
        );
    }
}

fn assert_openai_preset_contents(contents: &str) {
    for expected in [
        "kind = \"openai\"",
        "base_url = \"https://api.openai.com/v1\"",
        "api_key_env = \"OPENAI_API_KEY\"",
        "model = \"gpt-5.6-luna\"",
    ] {
        assert!(
            contents.contains(expected),
            "missing preset field: {expected}"
        );
    }
}

fn assert_invalid_answer_explanations(transcript: &str) {
    for (message, count) in [
        ("Enter a number from 1 to 5.", 1),
        ("That value must not be empty.", 1),
        (
            "That value must be an http:// or https:// URL with a host, no embedded credentials, and no query or fragment component.",
            3,
        ),
        ("That value must be an environment variable name", 2),
    ] {
        assert_eq!(transcript.matches(message).count(), count, "{message}");
    }
}

fn assert_stable_init_error_messages() {
    let io_error: InitError = io::Error::other("boom").into();
    for (error, expected) in [
        (
            InitError::Cancelled,
            "configuration cancelled; nothing was written",
        ),
        (
            InitError::Exists("p".to_string()),
            "configuration already exists at 'p'; 'ask configure apply' replaces regular files only",
        ),
        (io_error, "cannot continue configuration: boom"),
    ] {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn complete_dialogue_writes_a_loadable_default_profile() {
    let path = fresh_path();
    let (result, transcript) = drive(&path, COMPLETE);
    result.unwrap();
    let target = validate::document(&fs::read_to_string(&path).unwrap())
        .unwrap()
        .resolve()
        .unwrap();
    assert_local_openai_compatible_target(&target);
    assert_complete_dialogue_transcript(&transcript);
}

#[test]
fn openai_preset_supplies_endpoint_and_credential_defaults() {
    let path = fresh_path();
    let answers = "1\ngpt-5.6-luna\n\n\ny\n";
    drive(&path, answers).0.unwrap();
    assert_openai_preset_contents(&fs::read_to_string(&path).unwrap());
}

#[test]
fn replacement_prompt_and_profile_name_are_recorded() {
    let path = fresh_path();
    let answers = "5\nlocal\nhttps://example.test/v1\nKEY\nm\nBe terse.\nterse\nyes\n";
    drive(&path, answers).0.unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("default_profile = \"terse\""));
    assert!(contents.contains("[profiles.terse]"));
    let target = validate::document(&contents).unwrap().resolve().unwrap();
    assert_eq!(target.system_prompt, "Be terse.");
}

#[test]
fn end_of_input_cancels_at_every_prompt_without_writing() {
    let mut answers = String::new();
    for line in COMPLETE.split_inclusive('\n') {
        let path = fresh_path();
        let (result, _) = drive(&path, &answers);
        assert!(matches!(result, Err(InitError::Cancelled)), "{answers:?}");
        assert!(!path.exists());
        answers.push_str(line);
    }
}

#[test]
fn declining_confirmation_cancels_without_writing() {
    for refusal in ["n\n", "\n", "maybe\n"] {
        let path = fresh_path();
        let answers = COMPLETE.replace("y\n", refusal);
        let (result, _) = drive(&path, &answers);
        assert!(matches!(result, Err(InitError::Cancelled)));
        assert!(!path.exists());
    }
}

#[test]
fn invalid_answers_are_explained_and_asked_again() {
    let path = fresh_path();
    let answers = COMPLETE
        .replacen("5\n", "0\n5\n", 1)
        .replacen("local\n", "\nlocal\n", 1)
        .replacen("http://", "ftp://x\nhttp:// spaced\nhttp://\nhttp://", 1)
        .replacen("LOCAL_API_KEY\n", "1KEY\nBAD-NAME\nLOCAL_API_KEY\n", 1);
    let (result, transcript) = drive(&path, &answers);
    result.unwrap();
    assert_invalid_answer_explanations(&transcript);
}

#[test]
fn existing_file_is_refused_before_any_prompt() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "original").unwrap();
    let (result, transcript) = drive(&path, COMPLETE);
    assert!(matches!(result, Err(InitError::Exists(_))));
    assert!(transcript.is_empty());
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
}

#[test]
fn file_appearing_during_the_dialogue_is_not_overwritten() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "original").unwrap();
    let error = configure::create_new(&path, b"new").unwrap_err();
    assert!(matches!(failed(&path, &error), InitError::Exists(_)));
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
}

#[test]
fn unwritable_location_is_reported() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "").unwrap();
    let nested = path.join("config.toml");
    let (result, _) = drive(&nested, COMPLETE);
    let message = result.unwrap_err().to_string();
    assert!(message.starts_with("cannot write '"), "{message}");
}

#[test]
fn errors_have_stable_messages() {
    assert_stable_init_error_messages();
}
