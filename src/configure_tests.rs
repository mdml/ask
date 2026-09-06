use std::{
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;

static CASE_ID: AtomicUsize = AtomicUsize::new(0);

const COMPLETE: &str = "local\nhttp://127.0.0.1:1/v1\nfake-model\nLOCAL_API_KEY\n\n\ny\n";

fn fresh_path() -> PathBuf {
    let id = CASE_ID.fetch_add(1, Ordering::SeqCst);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/configure-unit-tests")
        .join(format!("{}-{id}", std::process::id()));
    dir.join("config.toml")
}

fn drive(path: &Path, answers: &str) -> (Result<(), ConfigureError>, String) {
    let mut input = Cursor::new(answers.as_bytes().to_vec());
    let mut output = Vec::new();
    let result = run(path, &mut input, &mut output);
    (result, String::from_utf8(output).unwrap())
}

#[test]
fn complete_dialogue_writes_a_loadable_default_profile() {
    let path = fresh_path();
    let (result, transcript) = drive(&path, COMPLETE);
    result.unwrap();
    let target = crate::config::parse(&fs::read_to_string(&path).unwrap())
        .unwrap()
        .resolve()
        .unwrap();
    assert_eq!(target.base_url, "http://127.0.0.1:1/v1");
    assert_eq!(target.model, "fake-model");
    assert_eq!(target.api_key_env, "LOCAL_API_KEY");
    assert_eq!(target.system_prompt, crate::DEFAULT_SYSTEM_PROMPT);
    assert!(transcript.contains(crate::DEFAULT_SYSTEM_PROMPT));
    assert!(transcript.contains("Write this configuration? [y/N]: Wrote '"));
}

#[test]
fn replacement_prompt_and_profile_name_are_recorded() {
    let path = fresh_path();
    let answers = "local\nhttps://example.test/v1\nm\nKEY\nBe terse.\nterse\nyes\n";
    drive(&path, answers).0.unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("default_profile = \"terse\""));
    assert!(contents.contains("[profiles.terse]"));
    let target = crate::config::parse(&contents).unwrap().resolve().unwrap();
    assert_eq!(target.system_prompt, "Be terse.");
}

#[test]
fn end_of_input_cancels_at_every_prompt_without_writing() {
    let mut answers = String::new();
    for line in COMPLETE.split_inclusive('\n') {
        let path = fresh_path();
        let (result, _) = drive(&path, &answers);
        assert!(
            matches!(result, Err(ConfigureError::Cancelled)),
            "{answers:?}"
        );
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
        assert!(matches!(result, Err(ConfigureError::Cancelled)));
        assert!(!path.exists());
    }
}

#[test]
fn invalid_answers_are_explained_and_asked_again() {
    let path = fresh_path();
    let answers = COMPLETE
        .replacen("local\n", "\nlocal\n", 1)
        .replacen("http://", "ftp://x\nhttp:// spaced\nhttp://\nhttp://", 1)
        .replacen("LOCAL_API_KEY\n", "1KEY\nBAD-NAME\nLOCAL_API_KEY\n", 1);
    let (result, transcript) = drive(&path, &answers);
    result.unwrap();
    assert_eq!(transcript.matches("A value is required.").count(), 1);
    assert_eq!(
        transcript
            .matches("Enter an http:// or https:// URL with a host and no embedded credentials.")
            .count(),
        3
    );
    assert_eq!(
        transcript
            .matches("Enter an environment variable name")
            .count(),
        2
    );
}

#[test]
fn existing_file_is_refused_before_any_prompt() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "original").unwrap();
    let (result, transcript) = drive(&path, COMPLETE);
    assert!(matches!(result, Err(ConfigureError::Exists(_))));
    assert!(transcript.is_empty());
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
}

#[test]
fn file_appearing_during_the_dialogue_is_not_overwritten() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "original").unwrap();
    assert!(matches!(
        write_new(&path, "new"),
        Err(ConfigureError::Exists(_))
    ));
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
    assert_eq!(
        ConfigureError::Cancelled.to_string(),
        "configuration cancelled; nothing was written"
    );
    assert_eq!(
        ConfigureError::Exists("p".to_string()).to_string(),
        "configuration already exists at 'p'; editing it is not yet supported"
    );
    let io_error: ConfigureError = io::Error::other("boom").into();
    assert_eq!(io_error.to_string(), "cannot continue configuration: boom");
}

#[test]
fn validators_accept_and_reject_expected_values() {
    assert!(http_url("https://a").is_ok());
    assert!(http_url("https://").is_err());
    assert!(env_var_name("_A1").is_ok());
    assert!(env_var_name("").is_err());
    assert!(env_var_name("A B").is_err());
}

#[test]
fn endpoint_validation_rejects_missing_hosts_and_embedded_credentials() {
    for endpoint in [
        "https:///v1",
        "https://?q=x",
        "https://user:secret@example.test/v1",
    ] {
        assert!(http_url(endpoint).is_err(), "{endpoint}");
    }
    assert!(http_url("http://[::1]:8080/v1").is_ok());
}
