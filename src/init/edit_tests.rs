use std::{
    collections::BTreeMap,
    env, fs,
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::init::{Console, Lookup, published, start};

static CASE_ID: AtomicUsize = AtomicUsize::new(0);

/// An existing configuration exactly as `ask init` renders it.
const EXISTING: &str = r#"default_profile = "default"
expire_history = true
history_days = 30

[providers.local]
kind = "openai-compatible"
base_url = "http://127.0.0.1:1/v1"
api_key_env = "LOCAL_API_KEY"
timeout_ms = 41

[providers.openai]
kind = "openai"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[profiles.default]
provider = "local"
model = "fake-model"
system_prompt = "Use terse tables."
max_output_tokens = 77

[profiles.spare]
provider = "openai"
model = "spare-model"
system_prompt = ""
"#;

fn existing(contents: &[u8]) -> PathBuf {
    let id = CASE_ID.fetch_add(1, Ordering::SeqCst);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/init-edit-unit-tests")
        .join(format!("{}-{id}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    fs::write(&path, contents).unwrap();
    path
}

/// Every variable is unset except the published-list override, which is
/// empty so no unit test requests the published list.
const UNSET: Lookup = |name| match name {
    published::URL_VARIABLE => Ok(String::new()),
    _ => Err(env::VarError::NotPresent),
};

fn drive(path: &Path, answers: &str) -> (Result<(), InitError>, String) {
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
    let result = runtime.block_on(start(path, console, UNSET));
    (result, String::from_utf8(output).unwrap())
}

/// Runs `answers` against [`EXISTING`] and returns the written file and transcript.
fn edited(answers: &str) -> (String, String) {
    let path = existing(EXISTING.as_bytes());
    let (result, transcript) = drive(&path, answers);
    result.unwrap();
    (fs::read_to_string(&path).unwrap(), transcript)
}

#[test]
fn the_summary_lists_the_default_and_every_profile() {
    let (_, transcript) = edited("4\n1\ny\n");
    for expected in [
        "Changing '",
        "Default profile: default\nProfiles:\n",
        "  default (default): provider local, model fake-model\n",
        "  spare: provider openai, model spare-model\n",
        " 4. Set the default profile\n",
    ] {
        assert!(transcript.contains(expected), "{expected}: {transcript}");
    }
}

#[test]
fn setting_the_default_changes_only_that_key() {
    let (written, transcript) = edited("4\n2\ny\n");
    assert_eq!(
        written,
        EXISTING.replace(
            "default_profile = \"default\"",
            "default_profile = \"spare\""
        )
    );
    assert!(!transcript.contains(REFORMAT_WARNING));
}

#[test]
fn changing_a_model_keeps_the_rest_and_says_threads_keep_their_profile() {
    let (written, transcript) = edited("3\n1\nnew-model\ny\n");
    assert_eq!(
        written,
        EXISTING.replace("model = \"fake-model\"", "model = \"new-model\"")
    );
    assert!(transcript.contains("LOCAL_API_KEY is not set; continuing without a key"));
    assert!(transcript.contains(NEW_THREADS_ONLY));
}

#[test]
fn a_profile_on_an_existing_provider_needs_an_unused_name() {
    let (written, transcript) = edited("2\n2\nwork-model\nBe brief.\nspare\nwork\ny\n");
    assert!(transcript.contains("Profile name [openai]: "));
    assert!(transcript.contains("That profile name is already used; choose another."));
    let expected = format!(
        "{EXISTING}\n[profiles.work]\nprovider = \"openai\"\nmodel = \"work-model\"\nsystem_prompt = \"Be brief.\"\n"
    );
    assert_eq!(written, expected);
}

#[test]
fn a_new_provider_gets_a_new_name_and_profile() {
    let (written, transcript) = edited("1\n1\nopenai\nwork\nw\n\n\ny\n");
    assert!(transcript.contains("A provider named 'openai' is already configured."));
    assert!(transcript.contains("That provider name is already used; choose another."));
    let config = validate::document(&written).unwrap();
    assert_eq!(config.profiles["work"].provider, "work");
    assert_eq!(config.providers["work"].kind, "openai");
    assert_eq!(config.default_profile, "default");
    for block in EXISTING.split("\n\n") {
        assert!(written.contains(block.trim_end()), "{block}: {written}");
    }
}

#[test]
fn a_reformatted_file_is_announced_before_confirmation() {
    let commented = format!("# keep me\n{EXISTING}");
    let path = existing(commented.as_bytes());
    let (result, transcript) = drive(&path, "4\n1\ny\n");
    result.unwrap();
    assert!(
        transcript.contains(&format!("{REFORMAT_WARNING}\nWrite this configuration?")),
        "{transcript}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), EXISTING);
}

#[test]
fn an_invalid_or_unreadable_file_is_left_alone_without_prompts() {
    for (contents, expected) in [
        (&b"SECRET_SENTINEL = 1\n"[..], "invalid configuration '"),
        (&b"\xffSECRET_SENTINEL"[..], "it is not valid UTF-8"),
    ] {
        let path = existing(contents);
        let (result, transcript) = drive(&path, "4\n1\ny\n");
        let message = result.unwrap_err().to_string();
        assert!(message.contains(expected), "{message}");
        assert!(message.ends_with(UNCHANGED), "{message}");
        assert!(!message.contains("SECRET_SENTINEL"), "{message}");
        assert!(transcript.is_empty(), "{transcript}");
        assert_eq!(fs::read(&path).unwrap(), contents);
    }
}

#[test]
fn end_of_input_or_refusal_at_any_prompt_leaves_the_file_alone() {
    let answers = "2\n2\nwork-model\n\nwork\ny\n";
    let mut prefix = String::new();
    for line in answers.split_inclusive('\n') {
        for input in [prefix.clone(), format!("{prefix}n\n")] {
            let path = existing(EXISTING.as_bytes());
            let (result, _) = drive(&path, &input);
            assert!(matches!(result, Err(InitError::Cancelled)), "{input:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), EXISTING);
        }
        prefix.push_str(line);
    }
}

#[test]
fn a_file_changed_after_it_was_read_is_not_overwritten() {
    let path = existing(EXISTING.as_bytes());
    let installed = configure::read_installed(&path).unwrap();
    fs::write(&path, "edited elsewhere\n").unwrap();
    let message = configure::replace_installed(&path, b"new", installed).unwrap_err();
    assert!(message.ends_with("changed after init read it; nothing was written"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "edited elsewhere\n");
    fs::remove_file(&path).unwrap();
    let message = configure::read_installed(&path).err().unwrap();
    assert!(message.ends_with("it no longer exists"), "{message}");
}

#[test]
fn a_result_the_validator_rejects_is_reported_not_written() {
    let config = Config {
        default_profile: "absent".to_string(),
        expire_history: false,
        history_days: None,
        providers: BTreeMap::new(),
        profiles: BTreeMap::new(),
    };
    let message = rendered(&config).unwrap_err().to_string();
    assert!(message.starts_with("internal error: "), "{message}");
}

#[test]
fn labels_neutralize_terminal_controls() {
    let provider = ProviderConfig {
        kind: "openai".to_string(),
        base_url: "http://x.test/v1".to_string(),
        api_key_env: None,
        timeout_ms: 1,
    };
    assert_eq!(
        provider_label("a\u{1b}[2Jb", &provider),
        "a [2Jb (openai, http://x.test/v1)"
    );
}
