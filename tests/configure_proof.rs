//! End-to-end proof of the configuration commands through the real binary.
//!
//! `ask init` creates a first configuration interactively or makes one change
//! to an existing one. `ask configure check`
//! validates a complete candidate document without writing, and
//! `ask configure apply` installs one. Every case here drives the installed
//! binary using redirected input, a PTY, or synchronized OS boundaries. Only
//! queries proving the written configuration is usable reach a fake provider.

mod support;

use std::{
    fs,
    io::{self, Write},
    path::Path,
    process::{Child, Command, Output, Stdio},
};

use support::{
    CREDENTIAL, MODEL_LIST_URL, command,
    fake_provider::{FakeProvider, RecordedRequest, Scenario},
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

/// A model list in the OpenAI shape, with a dated snapshot listed first and
/// an identifier that would clear the screen if it were printed.
const MODELS: &str = r#"{"object":"list","data":[{"id":"fake-model-2024-08-06"},{"id":"fake-model"},{"id":"other-model"},{"id":"\u001b[2Jevil"},{"id":"other-mini"}]}"#;
/// The key the PTY helper pastes at the hidden prompt.
const PASTED: &str = "pasted-secret-never-print";

/// Runs one `init_menu_process.py` scenario, optionally against `fake` and
/// with `LOCAL_API_KEY` set to the fixture credential.
#[cfg(unix)]
fn terminal_init(scenario: &str, fake: Option<&FakeProvider>, with_credential: bool) -> Output {
    let home = fresh_home();
    let mut helper = Command::new("python3");
    helper
        .arg("tests/support/init_menu_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(&home)
        .arg(scenario)
        .args(fake.map(FakeProvider::base_url))
        .env_remove("LOCAL_API_KEY");
    if with_credential {
        helper.env("LOCAL_API_KEY", CREDENTIAL);
    }
    let output = helper.output().unwrap();
    assert!(output.status.success(), "{scenario}: {}", stderr(&output));
    output
}

#[cfg(unix)]
#[test]
fn terminal_init_handles_selection_escape_and_keyboard_interrupt_and_restores_the_terminal() {
    for scenario in [
        "select",
        "escape",
        "ctrl-c",
        "hidden-escape",
        "local-escape",
    ] {
        terminal_init(scenario, None, false);
    }
}

#[cfg(unix)]
#[test]
fn terminal_init_filters_the_listed_models_and_verifies_the_choice() {
    let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
    terminal_init("filter", Some(&fake), true);
    let requests = fake.requests(2);
    let listing = &requests[0];
    assert_eq!(listing.method, "GET");
    assert_eq!(listing.path, "/v1/models");
    assert!(listing.authorization_is_fixture);
    let verification = &requests[1];
    assert_eq!(verification.model, "other-mini");
    let prompt = &verification.messages.last().unwrap().1;
    assert_eq!(prompt, ask::doctor::LIVE_PROMPT);
}

#[cfg(unix)]
#[test]
fn terminal_init_selects_a_local_server_and_filters_its_models_without_a_key() {
    let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
    terminal_init("local", Some(&fake), true);
    let requests = fake.requests(2);
    assert_eq!(requests[0].path, "/v1/models");
    assert_eq!(requests[1].model, "other-mini");
    assert_placeholder_only(&requests);
}

/// Every request carried the fixed placeholder and no environment credential.
fn assert_placeholder_only(requests: &[RecordedRequest]) {
    for request in requests {
        assert_eq!(request.header("authorization"), Some("Bearer no-key"));
        let wire = format!("{:?}{}", request.headers, request.body);
        assert!(
            !wire.contains(CREDENTIAL) && !wire.contains("OPENAI-ENV"),
            "{wire}"
        );
    }
}

#[cfg(unix)]
#[test]
fn terminal_init_uses_a_hidden_key_only_for_the_list_and_verification() {
    let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
    terminal_init("hidden", Some(&fake), false);
    let requests = fake.requests(2);
    assert_eq!(requests.len(), 2);
    let bearer = format!("Bearer {PASTED}");
    for request in &requests {
        assert_eq!(request.header("authorization"), Some(bearer.as_str()));
    }
    assert_eq!(requests[1].model, "typed-model");
}

#[cfg(unix)]
#[test]
fn terminal_init_escape_at_the_model_menu_writes_nothing() {
    let fake = FakeProvider::start(Scenario::Status(200, MODELS));
    terminal_init("model-escape", Some(&fake), true);
    assert_eq!(fake.requests(1).len(), 1);
}

#[test]
fn init_then_query_answers_through_the_fake_provider() {
    let fake = FakeProvider::start(Scenario::Stream);
    let home = fresh_home();
    let answers = format!(
        "9\nlocal\n{}\nLOCAL_API_KEY\nfake-model\nUse terse tables.\n\nn\ny\n",
        fake.base_url()
    );
    let transcript = succeeded(&interactive(&home, "init", &answers));
    for expected in [
        ask::DEFAULT_SYSTEM_PROMPT,
        "LOCAL_API_KEY is not set; continuing without a key, so the setup will not be verified.",
        "docs/guides/credentials.md",
    ] {
        assert!(transcript.contains(expected), "{transcript}");
    }
    let connections = fake.connections();
    assert_eq!(connections, 0, "redirected stdin never supplies a key");
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("api_key_env = \"LOCAL_API_KEY\""));

    let query = command(&home, true)
        .args(["what", "is", "2+2"])
        .output()
        .unwrap();
    assert!(query.status.success(), "{}", stderr(&query));
    assert_eq!(query.stdout, b"**4**\n");
    let request = fake.recorded().unwrap();
    assert_eq!(request.model, "fake-model");
    let (role, system_prompt) = &request.messages[0];
    assert_eq!(
        (role.as_str(), system_prompt.as_str()),
        ("system", "Use terse tables.")
    );
}

#[test]
fn init_offers_the_listed_models_and_verifies_with_the_environment_key() {
    let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
    let home = fresh_home();
    let answers = format!(
        "9\nlocal\n{}\nLOCAL_API_KEY\n0\n2\n\n\nn\ny\n",
        fake.base_url()
    );
    let transcript = succeeded(&keyed(&home, &answers));
    for expected in [
        "Using LOCAL_API_KEY from the environment (value not shown).",
        " 1. fake-model\n 2. other-model\n 3. other-mini\n 4. fake-model-2024-08-06\n 5. Enter a model identifier manually\n",
        "Enter a number from 1 to 5.",
        "warning: live check sends a minimal provider request that may incur cost",
        "Verified: the provider answered a minimal request.",
    ] {
        assert!(transcript.contains(expected), "{expected}: {transcript}");
    }
    assert!(!transcript.contains('\u{1b}'), "{transcript}");
    assert!(!transcript.contains(CREDENTIAL));
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("model = \"other-model\""), "{written}");
    assert!(!written.contains(CREDENTIAL));
    let requests = fake.requests(2);
    assert_eq!(
        (requests[0].method.as_str(), requests[0].path.as_str()),
        ("GET", "/v1/models")
    );
    assert!(
        requests
            .iter()
            .all(|request| request.authorization_is_fixture)
    );
    assert_eq!(requests[1].model, "other-model");
}

/// Answers for a local server: preset `choice`, endpoint `url`, then the
/// listed model number, defaults for the prompt and profile, and write.
fn local_answers(choice: usize, url: &str, tail: &str) -> String {
    format!("8\n{choice}\n{url}\n{tail}")
}

/// Runs `ask init` with unrelated credential variables set.
fn keyed_elsewhere(home: &Path, answers: &str) -> Output {
    drive(
        command(home, true)
            .env("OPENAI_API_KEY", "OPENAI-ENV-SECRET")
            .arg("init"),
        answers,
    )
}

#[test]
fn init_writes_each_local_preset_and_verifies_it_without_a_key() {
    for (choice, name, default_url) in [
        (1, "ollama", "http://localhost:11434/v1"),
        (2, "lmstudio", "http://localhost:1234/v1"),
        (3, "llamacpp", "http://localhost:8080/v1"),
    ] {
        let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
        let home = fresh_home();
        let answers = local_answers(choice, &fake.base_url(), "2\n\n\nn\ny\n");
        let transcript = succeeded(&keyed_elsewhere(&home, &answers));
        for expected in [
            format!("Endpoint base URL [{default_url}]: "),
            "Requesting the model list from".to_string(),
            "Sending a minimal request to the local server to verify it.".to_string(),
            "Verified: the provider answered a minimal request.".to_string(),
        ] {
            assert!(transcript.contains(&expected), "{expected}: {transcript}");
        }
        assert!(!transcript.contains("incur cost"), "{transcript}");
        assert!(!transcript.contains("Credential variable"), "{transcript}");
        let expected = format!(
            "[providers.{name}]\nkind = \"openai-compatible\"\nbase_url = \"{}\"\ntimeout_ms = 120000\n",
            fake.base_url()
        );
        let written = fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(written.contains(&expected), "{written}");
        assert!(!written.contains("api_key_env"), "{written}");
        let requests = fake.requests(2);
        assert_eq!(requests[0].path, "/v1/models");
        assert_eq!(requests[1].model, "other-model");
        assert_placeholder_only(&requests);
    }
}

#[test]
fn a_query_after_local_init_works_with_no_key() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Status(200, MODELS),
        Scenario::Stream,
        Scenario::Stream,
    ]);
    let home = fresh_home();
    let answers = local_answers(1, &fake.base_url(), "1\n\n\nn\ny\n");
    succeeded(&interactive(&home, "init", &answers));
    let query = command(&home, false).arg("what").output().unwrap();
    assert!(query.status.success(), "{}", stderr(&query));
    assert_eq!(query.stdout, b"**4**\n");
    let requests = fake.requests(3);
    assert_eq!(requests[2].model, "fake-model");
    assert_placeholder_only(&requests);
}

#[test]
fn an_unreachable_local_server_falls_back_to_manual_entry_with_the_start_hint() {
    let url = "http://127.0.0.1:1/v1";
    for (choice, hint) in [
        (1, "`ollama serve`"),
        (2, "`lms server start`"),
        (3, "`llama-server -m <model.gguf>`"),
    ] {
        let home = fresh_home();
        let answers = local_answers(choice, url, "typed-model\n\n\ny\nn\ny\n");
        let transcript = succeeded(&interactive(&home, "init", &answers));
        let notice = format!("Cannot list models from {url} (");
        assert!(transcript.contains(&notice), "{transcript}");
        assert!(transcript.contains(hint), "{transcript}");
        assert!(transcript.contains("Model identifier (free text sent to the provider): "));
        let written = fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(written.contains("model = \"typed-model\""), "{written}");
    }
}

#[test]
fn a_keyless_custom_endpoint_lists_models_and_verifies_too() {
    let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
    let home = fresh_home();
    let answers = format!("9\nlocal\n{}\n\n1\n\n\nn\ny\n", fake.base_url());
    let transcript = succeeded(&keyed_elsewhere(&home, &answers));
    assert!(
        transcript.contains("Verified: the provider answered"),
        "{transcript}"
    );
    assert_placeholder_only(&fake.requests(2));
}

#[test]
fn an_empty_local_model_list_falls_back_to_manual_entry() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Status(200, r#"{"object":"list","data":[]}"#),
        Scenario::Stream,
    ]);
    let home = fresh_home();
    let answers = local_answers(3, &fake.base_url(), "typed-model\n\n\nn\ny\n");
    let transcript = succeeded(&interactive(&home, "init", &answers));
    assert!(
        transcript.contains("the server listed no models"),
        "{transcript}"
    );
    assert!(
        transcript.contains("`llama-server -m <model.gguf>`"),
        "{transcript}"
    );
}

/// What init prints when verification fails before it writes.
const ASKED_BEFORE_WRITING: [&str; 2] = [
    "Verification failed: ",
    "Write the configuration anyway? [y/N]: ",
];

#[test]
fn a_failed_local_verification_asks_before_writing() {
    for (decision, written) in [("n\n", false), ("y\nn\ny\n", true)] {
        let fake =
            FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Unauthorized]);
        let home = fresh_home();
        let answers = local_answers(1, &fake.base_url(), &format!("1\n\n\n{decision}"));
        let output = interactive(&home, "init", &answers);
        let transcript = stderr(&output);
        for expected in ASKED_BEFORE_WRITING {
            assert!(transcript.contains(expected), "{transcript}");
        }
        assert_eq!(output.status.success(), written, "{transcript}");
        assert_eq!(home.join("config.toml").exists(), written);
    }
}

#[test]
fn init_end_of_input_cancels_at_every_local_prompt_without_writing() {
    let mut prefix = String::new();
    let answers = "8\n2\n{url}\n1\n\n\n";
    for line in answers.split_inclusive('\n').chain(["unused\n"]) {
        let fake =
            FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Unauthorized]);
        let home = fresh_home();
        let transcript = failed(&interactive(
            &home,
            "init",
            &prefix.replace("{url}", &fake.base_url()),
        ));
        assert!(transcript.ends_with(CANCELLED), "{prefix:?}: {transcript}");
        assert!(!home.join("config.toml").exists());
        prefix.push_str(line);
    }
}

#[test]
fn init_falls_back_to_manual_entry_when_the_list_fails() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Status(500, r#"{"error":"down"}"#),
        Scenario::Stream,
    ]);
    let home = fresh_home();
    let answers = format!(
        "9\nlocal\n{}\nLOCAL_API_KEY\ntyped-model\n\n\nn\ny\n",
        fake.base_url()
    );
    let transcript = succeeded(&keyed(&home, &answers));
    assert!(
        transcript.contains("Cannot list models (provider returned HTTP status 500"),
        "{transcript}"
    );
    assert!(transcript.contains("Model identifier (free text sent to the provider): "));
    assert_eq!(fake.requests(2)[1].model, "typed-model");
}

#[test]
fn init_asks_before_writing_after_a_failed_verification() {
    for (decision, written) in [("n\n", false), ("y\nn\ny\n", true)] {
        let fake =
            FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Unauthorized]);
        let home = fresh_home();
        let answers = format!(
            "9\nlocal\n{}\nLOCAL_API_KEY\n1\n\n\n{decision}",
            fake.base_url()
        );
        let output = keyed(&home, &answers);
        let transcript = stderr(&output);
        for expected in ASKED_BEFORE_WRITING {
            assert!(transcript.contains(expected), "{transcript}");
        }
        assert!(!transcript.contains(CREDENTIAL));
        assert_eq!(output.status.success(), written, "{transcript}");
        assert_eq!(home.join("config.toml").exists(), written);
    }
}

#[test]
fn init_adds_a_second_provider_with_its_own_profile() {
    let fake = FakeProvider::sequence(vec![
        Scenario::Status(200, MODELS),
        Scenario::Stream,
        Scenario::Status(200, MODELS),
        Scenario::Stream,
    ]);
    let home = fresh_home();
    let url = fake.base_url();
    let answers = format!(
        "9\nlocal\n{url}\nLOCAL_API_KEY\n1\n\n\ny\n9\nlocal\nsecond\n{url}\nLOCAL_API_KEY\n3\n\n\nn\ny\n"
    );
    let transcript = succeeded(&keyed(&home, &answers));
    assert!(transcript.contains("That provider name is already used; choose another."));
    assert!(transcript.contains("Profile name [second]: "));
    let config = fs::read_to_string(home.join("config.toml")).unwrap();
    for expected in [
        "default_profile = \"default\"",
        "[providers.local]",
        "[providers.second]",
        "[profiles.default]\nprovider = \"local\"\nmodel = \"fake-model\"",
        "[profiles.second]\nprovider = \"second\"\nmodel = \"other-mini\"",
    ] {
        assert!(config.contains(expected), "{expected}: {config}");
    }
    assert_eq!(fake.requests(4).len(), 4);
}

#[test]
fn init_end_of_input_cancels_at_every_keyed_prompt_without_writing() {
    let answers = "9\nlocal\n{url}\nLOCAL_API_KEY\n1\n\n\n";
    let mut prefix = String::new();
    // The trailing entry makes the complete answers the last prefix tried.
    for line in answers.split_inclusive('\n').chain(["unused\n"]) {
        let fake =
            FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Unauthorized]);
        let home = fresh_home();
        let input = prefix.replace("{url}", &fake.base_url());
        let transcript = failed(&keyed(&home, &input));
        assert!(transcript.ends_with(CANCELLED), "{input:?}: {transcript}");
        assert!(!home.join("config.toml").exists());
        prefix.push_str(line);
    }
}

#[test]
fn init_alias_i_creates_the_same_file() {
    let home = fresh_home();
    let answers = "9\nlocal\nhttp://127.0.0.1:1/v1\nLOCAL_API_KEY\nfake-model\n\n\nn\ny\n";
    succeeded(&interactive(&home, "i", answers));
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("default_profile = \"default\""));
    assert!(written.contains("[profiles.default]"));
}

#[test]
fn init_keeps_the_default_system_prompt_when_the_answer_is_empty() {
    let home = fresh_home();
    let answers = "9\nlocal\nhttp://127.0.0.1:1/v1\nLOCAL_API_KEY\nfake-model\n\n\nn\ny\n";
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
    let answers = "9\nlocal\nhttp://127.0.0.1:1/v1\n";
    let transcript = failed(&interactive(&home, "init", answers));
    assert!(transcript.ends_with(CANCELLED), "{transcript}");
    assert!(!home.join("config.toml").exists());
}

#[test]
fn init_asks_again_after_an_invalid_answer() {
    let home = fresh_home();
    let answers =
        "9\nlocal\nnot-a-url\nhttp://127.0.0.1:1/v1\n9KEY\nLOCAL_API_KEY\nfake-model\n\n\nn\ny\n";
    let transcript = succeeded(&interactive(&home, "init", answers));
    assert!(transcript.contains(
        "That value must be an http:// or https:// URL with a host, no embedded credentials, and no query or fragment component."
    ));
    assert!(transcript.contains("That value must be an environment variable name"));
    assert!(home.join("config.toml").exists());
}

#[test]
fn init_openai_preset_writes_supported_kind_and_defaults() {
    let home = fresh_home();
    let answers = "1\ngpt-5.6-luna\n\n\nn\ny\n";
    let transcript = succeeded(&interactive(&home, "init", answers));
    assert!(transcript.contains(
        "OPENAI_API_KEY is not set; continuing without a key, so the setup will not be verified."
    ));
    assert!(
        !transcript.contains("published model list"),
        "an empty {MODEL_LIST_URL} disables it: {transcript}"
    );
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("kind = \"openai\""));
    assert!(written.contains("base_url = \"https://api.openai.com/v1\""));
    assert!(written.contains("api_key_env = \"OPENAI_API_KEY\""));
}

#[test]
fn help_and_version_run_without_configuration() {
    let home = fresh_home();
    let help = command(&home, false).arg("help").output().unwrap();
    assert!(help.status.success(), "{}", stderr(&help));
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("ask init creates the first configuration interactively or changes")
    );
    assert!(String::from_utf8_lossy(&help.stderr).is_empty());

    let version = command(&home, false).args(["--version"]).output().unwrap();
    assert!(version.status.success(), "{}", stderr(&version));
    assert_eq!(String::from_utf8(version.stdout).unwrap(), "ask 0.1.0\n");
    assert!(String::from_utf8_lossy(&version.stderr).is_empty());
}

#[cfg(unix)]
#[test]
fn symlink_destinations_are_refused_before_initialization_prompts() {
    use std::os::unix::fs::symlink;
    for dangling in [true, false] {
        let home = fresh_home();
        let target = home.join("target.toml");
        if !dangling {
            fs::write(&target, CANDIDATE).unwrap();
        }
        symlink(&target, home.join("config.toml")).unwrap();
        let message = failed(&interactive(&home, "init", ""));
        assert!(message.contains("already exists"), "{message}");
        assert!(message.contains("regular files only"), "{message}");
        assert_eq!(message.lines().count(), 1);
        let message = failed(&configure(&home, &["configure", "apply", "-"], CANDIDATE));
        assert!(message.contains("cannot inspect"), "{message}");
        assert!(message.contains("not a symlink"), "{message}");
        assert_eq!(target.exists(), !dangling);
        if !dangling {
            assert_eq!(fs::read_to_string(target).unwrap(), CANDIDATE);
        }
    }
}

#[cfg(unix)]
#[test]
fn init_failed_disk_write_leaves_no_config_and_allows_retry() {
    let home = fresh_home();
    let answers = format!(
        "9\nlocal\nhttp://127.0.0.1:1/v1\nLOCAL_API_KEY\nfake-model\n{}\n\nn\ny\n",
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
            "ask: standard input is not a valid configuration: providers[2].kind has an unsupported kind; supported kinds are 'openai', 'anthropic', 'gemini', 'openrouter', 'openai-compatible'\n",
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

/// Runs `verb` with redirected `answers` and no credential in the environment.
fn interactive(home: &Path, verb: &str, answers: &str) -> Output {
    drive(command(home, false).arg(verb), answers)
}

/// Runs `ask init` with redirected `answers` and the fixture credential set.
fn keyed(home: &Path, answers: &str) -> Output {
    drive(command(home, true).arg("init"), answers)
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
        .env(MODEL_LIST_URL, "")
        .env_remove("LOCAL_API_KEY")
        // The disk limit would also truncate this child's coverage profile.
        .env("LLVM_PROFILE_FILE", "/dev/null");
    limited
}

fn drive(command: &mut Command, input: &str) -> Output {
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    finish(child, input)
}

fn finish(mut child: Child, input: &str) -> Output {
    write_input(child.stdin.take().unwrap(), input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn write_input(mut stdin: impl Write, input: &[u8]) -> io::Result<()> {
    match stdin.write_all(input) {
        // A command may refuse the operation before reading stdin. Its output
        // and exit status still need to reach the proof's assertions.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

#[test]
fn input_write_errors_other_than_broken_pipe_are_returned() {
    let error = write_input(&mut [][..], b"input").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WriteZero);
}

#[cfg(unix)]
#[test]
fn closed_child_stdin_preserves_output_and_exit_status() {
    for status in [0, 7] {
        let mut child = Command::new("sh")
            .args([
                "-c",
                "exec 0<&-; printf 'child stdout'; printf 'child stderr' >&2; exit \"$1\"",
                "closed-stdin",
                &status.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Keep the writer open across wait(), which otherwise closes it. Waiting
        // guarantees the child's reader is closed before finish() writes bytes.
        let stdin = child.stdin.take().unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(status));
        child.stdin = Some(stdin);

        let output = finish(child, "unread input");
        assert_eq!(output.status.code(), Some(status));
        assert_eq!(output.stdout, b"child stdout");
        assert_eq!(output.stderr, b"child stderr");
    }
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
        "edit_changed",
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
