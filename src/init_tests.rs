use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::{config::Target, validate};

static CASE_ID: AtomicUsize = AtomicUsize::new(0);

/// A key only the `keyed` lookup returns; it must never reach a transcript.
const UNIT_SECRET: &str = "unit-secret-never-print";

const COMPLETE: &str = "9\nlocal\nhttp://127.0.0.1:1/v1\nLOCAL_API_KEY\nfake-model\n\n\nn\ny\n";

fn fresh_path() -> PathBuf {
    let id = CASE_ID.fetch_add(1, Ordering::SeqCst);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/init-unit-tests")
        .join(format!("{}-{id}", std::process::id()));
    dir.join("config.toml")
}

fn unset(_: &str) -> Result<String, env::VarError> {
    Err(env::VarError::NotPresent)
}

fn keyed(_: &str) -> Result<String, env::VarError> {
    Ok(UNIT_SECRET.to_string())
}

fn not_unicode(_: &str) -> Result<String, env::VarError> {
    Err(env::VarError::NotUnicode("x".into()))
}

fn drive_with(path: &Path, answers: &str, lookup: Lookup) -> (Result<(), InitError>, String) {
    let mut input = Cursor::new(answers.as_bytes().to_vec());
    let mut output = Vec::new();
    let console = Console {
        input: &mut input,
        output: &mut output,
        menus: false,
        attended: false,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(start(path, console, lookup));
    (result, String::from_utf8(output).unwrap())
}

fn drive(path: &Path, answers: &str) -> (Result<(), InitError>, String) {
    drive_with(path, answers, unset)
}

fn assert_local_openai_compatible_target(target: &Target) {
    assert_eq!(
        (
            target.base_url.as_str(),
            target.model.as_str(),
            target.api_key_env.as_deref().unwrap(),
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
        "LOCAL_API_KEY is not set; skipping the model list and verification.",
        "read -rs LOCAL_API_KEY",
        "docs/guides/credentials.md",
        "ask 'what is 2+2'",
        "Write this configuration? [y/N]: Wrote '",
    ] {
        assert!(
            transcript.contains(expected),
            "missing dialogue text: {expected}"
        );
    }
}

fn assert_preset_contents(contents: &str, fields: [&str; 4]) {
    for field in fields {
        assert!(contents.contains(field), "missing preset field: {field}");
    }
}

fn assert_invalid_answer_explanations(transcript: &str) {
    for (message, count) in [
        ("Enter a number from 1 to 9.", 1),
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
fn presets_supply_endpoint_and_credential_defaults() {
    for (choice, fields) in [
        (
            "1",
            [
                "kind = \"openai\"",
                "base_url = \"https://api.openai.com/v1\"",
                "api_key_env = \"OPENAI_API_KEY\"",
                "[providers.openai]",
            ],
        ),
        (
            "5",
            [
                "kind = \"openai-compatible\"",
                "base_url = \"https://api.groq.com/openai/v1\"",
                "api_key_env = \"GROQ_API_KEY\"",
                "[providers.groq]",
            ],
        ),
        (
            "6",
            [
                "kind = \"openai-compatible\"",
                "base_url = \"https://api.cerebras.ai/v1\"",
                "api_key_env = \"CEREBRAS_API_KEY\"",
                "[providers.cerebras]",
            ],
        ),
        (
            "7",
            [
                "kind = \"openai-compatible\"",
                "base_url = \"https://api.x.ai/v1\"",
                "api_key_env = \"XAI_API_KEY\"",
                "[providers.xai]",
            ],
        ),
    ] {
        let path = fresh_path();
        let answers = format!("{choice}\nsome-model\n\n\nn\ny\n");
        drive(&path, &answers).0.unwrap();
        assert_preset_contents(&fs::read_to_string(&path).unwrap(), fields);
    }
}

#[test]
fn replacement_prompt_and_profile_name_are_recorded() {
    let path = fresh_path();
    let answers = "9\nlocal\nhttps://example.test/v1\nKEY\nm\nBe terse.\nterse\nn\nyes\n";
    drive(&path, answers).0.unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("default_profile = \"terse\""));
    assert!(contents.contains("[profiles.terse]"));
    let target = validate::document(&contents).unwrap().resolve().unwrap();
    assert_eq!(target.system_prompt, "Be terse.");
}

#[test]
fn another_provider_gets_its_own_uniquely_named_profile() {
    let path = fresh_path();
    let answers = concat!(
        "1\nfirst-model\n\n\ny\n",
        "1\nopenai\nwork\nsecond-model\n\ndefault\nwork-profile\nn\ny\n"
    );
    let (result, transcript) = drive(&path, answers);
    result.unwrap();
    assert!(transcript.contains("A provider named 'openai' is already configured."));
    assert!(transcript.contains("That provider name is already used; choose another."));
    assert!(transcript.contains("Profile name [work]: "));
    assert!(transcript.contains("That profile name is already used; choose another."));
    assert!(transcript.contains("Supply OPENAI_API_KEY only"));
    let config = validate::document(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(config.default_profile, "default");
    assert_eq!(config.profiles["default"].model, "first-model");
    assert_eq!(config.profiles["work-profile"].provider, "work");
    assert_eq!(config.profiles["work-profile"].model, "second-model");
}

#[test]
fn a_later_profile_defaults_to_the_provider_name() {
    let path = fresh_path();
    let answers = "2\nm\n\n\ny\n3\nm\n\n\nn\ny\n";
    drive(&path, answers).0.unwrap();
    let config = validate::document(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(config.default_profile, "default");
    assert_eq!(config.profiles["default"].provider, "anthropic");
    assert_eq!(config.profiles["gemini"].provider, "gemini");
}

#[test]
fn an_environment_key_lists_models_and_verifies_without_being_shown() {
    let path = fresh_path();
    let answers = "9\nlocal\nhttp://127.0.0.1:1/v1\nKEY\nfake-model\n\n\nyes\nn\ny\n";
    let (result, transcript) = drive_with(&path, answers, keyed);
    result.unwrap();
    for expected in [
        "Using KEY from the environment (value not shown).",
        "Requesting the model list from http://127.0.0.1:1/v1.",
        "Cannot list models (",
        "warning: live check sends a minimal provider request that may incur cost",
        "Verification failed: ",
        "Write the configuration anyway? [y/N]: ",
    ] {
        assert!(transcript.contains(expected), "{expected}: {transcript}");
    }
    assert!(!transcript.contains(UNIT_SECRET));
    assert!(!fs::read_to_string(&path).unwrap().contains(UNIT_SECRET));
}

#[test]
fn declining_after_a_failed_verification_writes_nothing() {
    let path = fresh_path();
    let answers = "9\nlocal\nhttp://127.0.0.1:1/v1\nKEY\nfake-model\n\n\nn\n";
    let (result, transcript) = drive_with(&path, answers, keyed);
    assert!(matches!(result, Err(InitError::Cancelled)), "{transcript}");
    assert!(!path.exists());
}

#[test]
fn a_key_that_is_not_unicode_is_ignored() {
    let path = fresh_path();
    let (result, transcript) = drive_with(&path, COMPLETE, not_unicode);
    result.unwrap();
    assert!(transcript.contains("LOCAL_API_KEY is set but is not valid Unicode; ignoring it."));
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
        let answers = COMPLETE.replace("n\ny\n", &format!("n\n{refusal}"));
        let (result, _) = drive(&path, &answers);
        assert!(matches!(result, Err(InitError::Cancelled)));
        assert!(!path.exists());
    }
}

#[test]
fn invalid_answers_are_explained_and_asked_again() {
    let path = fresh_path();
    let answers = COMPLETE
        .replacen("9\n", "0\n9\n", 1)
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
fn diagnostics_are_collapsed_to_one_printable_line() {
    assert_eq!(one_line("a\n\u{1b}[2J b\r\n\tc "), "a [2J b c");
}

#[test]
fn errors_have_stable_messages() {
    assert_stable_init_error_messages();
}

#[test]
fn an_empty_credential_answer_means_no_credential() {
    let path = fresh_path();
    let answers = "9\nlocal\nhttp://127.0.0.1:1/v1\n\nfake-model\n\n\ny\nn\ny\n";
    let (result, transcript) = drive(&path, answers);
    result.unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    assert!(!contents.contains("api_key_env"), "{contents}");
    let target = validate::document(&contents).unwrap().resolve().unwrap();
    assert_eq!(target.api_key_env, None);
    assert!(transcript.contains("empty means no credential"));
    assert!(!transcript.contains("read -rs"));
}

/// Answers for a local preset at an unreachable endpoint: the list fails, the
/// model is typed, verification fails and is accepted, then the file is written.
fn local_answers(choice: &str, endpoint: &str) -> String {
    format!("8\n{choice}\n{endpoint}\ntyped-model\n\n\ny\nn\ny\n")
}

#[test]
fn local_presets_write_a_keyless_target_with_a_long_timeout() {
    for (choice, name, url) in [
        ("1", "ollama", "http://localhost:11434/v1"),
        ("2", "lmstudio", "http://localhost:1234/v1"),
        ("3", "llamacpp", "http://localhost:8080/v1"),
    ] {
        let path = fresh_path();
        let answers = local_answers(choice, "http://127.0.0.1:1/v1");
        let (result, transcript) = drive(&path, &answers);
        result.unwrap();
        assert!(transcript.contains(&format!("[{url}]: ")), "{transcript}");
        let contents = fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains(&format!("[providers.{name}]")),
            "{contents}"
        );
        assert!(!contents.contains("api_key_env"), "{contents}");
        let target = validate::document(&contents).unwrap().resolve().unwrap();
        assert_eq!(target.kind, "openai-compatible");
        assert_eq!(target.timeout_ms, 120_000);
        assert_eq!(target.base_url, "http://127.0.0.1:1/v1");
    }
}

#[test]
fn an_unreachable_local_server_names_the_endpoint_and_how_to_start_it() {
    let path = fresh_path();
    let (result, transcript) = drive(&path, &local_answers("1", "http://127.0.0.1:1/v1"));
    result.unwrap();
    for expected in [
        "Cannot list models from http://127.0.0.1:1/v1 (",
        "Ollama is usually started with `ollama serve`. Enter the identifier manually.",
        "Sending a minimal request to the local server to verify it.",
        "Verification failed: ",
        "Write the configuration anyway? [y/N]: ",
    ] {
        assert!(transcript.contains(expected), "{expected}: {transcript}");
    }
    assert!(!transcript.contains("incur cost"), "{transcript}");
}

#[test]
fn a_keyless_custom_endpoint_gets_no_start_hint() {
    let notice = model::keyless_notice("http://x/v1", None, None);
    assert_eq!(
        notice,
        "Cannot list models from http://x/v1 (the server listed no models). Enter the identifier manually."
    );
}

#[test]
fn a_local_preset_name_in_use_asks_for_another() {
    let path = fresh_path();
    let answers = "8\n1\nhttp://127.0.0.1:1/v1\nm\n\n\ny\ny\n8\n1\nhttp://127.0.0.1:1/v1\nollama\nsecond\nm\n\n\ny\nn\ny\n";
    let (result, transcript) = drive(&path, answers);
    result.unwrap();
    assert!(transcript.contains("A provider named 'ollama' is already configured."));
    assert!(transcript.contains("That provider name is already used; choose another."));
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("[providers.second]"), "{contents}");
}

#[test]
fn the_local_endpoint_question_takes_enter_as_the_default_and_validates_typed_urls() {
    let mut input = Cursor::new(b"\nftp://x\nhttp://example.test/v1\n".to_vec());
    let mut output = Vec::new();
    let console = Console {
        input: &mut input,
        output: &mut output,
        menus: false,
        attended: false,
    };
    let mut dialogue = Dialogue {
        console,
        lookup: unset,
    };
    assert_eq!(
        dialogue.endpoint_or("? ", "http://d/v1").unwrap(),
        "http://d/v1"
    );
    assert_eq!(
        dialogue.endpoint_or("? ", "http://d/v1").unwrap(),
        "http://example.test/v1"
    );
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("That value must be an http://")
    );
}

#[test]
fn end_of_input_at_the_local_server_menu_writes_nothing() {
    let path = fresh_path();
    let (result, _) = drive(&path, "8\n");
    assert!(matches!(result, Err(InitError::Cancelled)));
    assert!(!path.exists());
}
