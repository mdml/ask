//! End-to-end proof of `ask init` on an existing configuration, through the
//! real binary with redirected input and a PTY against a fake provider.

mod support;

use std::{
    fs,
    io::{self, Write},
    path::Path,
    process::{Child, Command, Output, Stdio},
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

/// An existing configuration exactly as `ask init` renders it, carrying every
/// value `init` does not manage. `{url}` is replaced with the fake provider.
const EXISTING: &str = r#"default_profile = "default"
expire_history = true
history_days = 30

[providers.local]
kind = "openai-compatible"
base_url = "{url}"
api_key_env = "LOCAL_API_KEY"
timeout_ms = 41000

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

const REFORMAT_WARNING: &str = "warning: comments and formatting in the existing file will not be preserved; 'ask configure apply' keeps them.";

/// Matches no block of [`EXISTING`], for a change that only adds tables.
const ADDED: &str = "\0";

/// One redirected answer set per edit action, without a key in the
/// environment, with the text of the block each one changes.
const EDIT_ANSWERS: [(&str, &str); 4] = [
    ("1\n2\nclaude-model\n\n\ny\n", ADDED),
    ("2\n1\nm\n\nwork\ny\n", ADDED),
    ("3\n1\nm\ny\n", "fake-model"),
    ("4\n2\ny\n", "default_profile"),
];

/// [`EXISTING`] installed in a fresh home, pointing at a fake provider if any.
struct Installed {
    home: std::path::PathBuf,
    original: String,
}

impl Installed {
    fn new(fake: Option<&FakeProvider>) -> Self {
        let home = fresh_home();
        let url = fake.map_or_else(|| "http://127.0.0.1:1/v1".into(), FakeProvider::base_url);
        let original = EXISTING.replace("{url}", &url);
        fs::write(home.join("config.toml"), &original).unwrap();
        Self { home, original }
    }

    fn written(&self) -> String {
        fs::read_to_string(self.home.join("config.toml")).unwrap()
    }

    /// Asserts every block of the original survives except the one containing
    /// `changed`, and returns the written file.
    fn kept(&self, changed: &str) -> String {
        let written = self.written();
        for block in self
            .original
            .split("\n\n")
            .filter(|block| !block.contains(changed))
        {
            assert!(written.contains(block.trim_end()), "{block}: {written}");
        }
        written
    }
}

fn assert_mentions(transcript: &str, expected: &[&str]) {
    for text in expected {
        assert!(transcript.contains(text), "{text}: {transcript}");
    }
}

#[test]
fn init_edit_sets_the_default_profile_without_a_reformat_warning() {
    let installed = Installed::new(None);
    let transcript = succeeded(&interactive(&installed.home, "init", EDIT_ANSWERS[3].0));
    assert_mentions(
        &transcript,
        &[
            "Default profile: default\nProfiles:\n  default (default): provider local, model fake-model\n  spare: provider openai, model spare-model\n",
            "Choose a change [1-4]: ",
            "Select the default profile [1-2]: ",
            "Configuration to write:",
        ],
    );
    assert!(!transcript.contains(REFORMAT_WARNING), "{transcript}");
    let expected = installed.original.replace(
        "default_profile = \"default\"",
        "default_profile = \"spare\"",
    );
    assert_eq!(installed.written(), expected);
}

#[test]
fn init_edit_warns_that_comments_and_formatting_are_not_preserved() {
    let home = fresh_home();
    fs::write(home.join("config.toml"), CANDIDATE).unwrap();
    let transcript = succeeded(&interactive(&home, "i", "4\n1\ny\n"));
    let warning = format!("{REFORMAT_WARNING}\nWrite this configuration? [y/N]: ");
    assert_mentions(&transcript, &[&warning]);
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(!written.contains('#'), "{written}");
    assert_mentions(
        &written,
        &["timeout_ms = 41", "system_prompt = \"Use terse tables.\""],
    );
}

#[test]
fn init_edit_changes_a_model_from_the_list_and_verifies_it() {
    let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
    let installed = Installed::new(Some(&fake));
    let transcript = succeeded(&keyed(&installed.home, "3\n1\n2\ny\n"));
    assert_mentions(
        &transcript,
        &[
            "Using LOCAL_API_KEY from the environment (value not shown).",
            "The new model applies to new threads only; existing threads keep the profile they were created with.",
            "Verified: the provider answered a minimal request.",
        ],
    );
    assert!(!transcript.contains(CREDENTIAL));
    let expected = installed
        .original
        .replace("model = \"fake-model\"", "model = \"other-model\"");
    assert_eq!(installed.written(), expected);
    assert_eq!(fake.requests(2)[1].model, "other-model");
}

#[test]
fn init_edit_adds_a_profile_on_an_existing_provider_with_an_unused_name() {
    let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
    let installed = Installed::new(Some(&fake));
    let answers = "2\n1\n3\nBe brief.\ndefault\nwork\ny\n";
    let transcript = succeeded(&keyed(&installed.home, answers));
    assert_mentions(
        &transcript,
        &[
            "Profile name [local]: ",
            "That profile name is already used; choose another.",
            "Verified: the provider answered a minimal request.",
        ],
    );
    let expected = format!(
        "{}\n[profiles.work]\nprovider = \"local\"\nmodel = \"other-mini\"\nsystem_prompt = \"Be brief.\"\n",
        installed.original
    );
    assert_eq!(installed.written(), expected);
    assert_eq!(fake.requests(2)[1].model, "other-mini");
}

#[test]
fn init_edit_adds_a_provider_with_an_unused_name_and_its_own_profile() {
    let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
    let installed = Installed::new(Some(&fake));
    let answers = format!(
        "1\n9\nlocal\nsecond\n{}\nLOCAL_API_KEY\n1\n\n\ny\n",
        fake.base_url()
    );
    let transcript = succeeded(&keyed(&installed.home, &answers));
    assert_mentions(
        &transcript,
        &[
            "That provider name is already used; choose another.",
            "Profile name [second]: ",
        ],
    );
    assert_mentions(
        &installed.kept(ADDED),
        &[
            "[providers.second]\nkind = \"openai-compatible\"",
            "[profiles.second]\nprovider = \"second\"\nmodel = \"fake-model\"",
        ],
    );
    assert_eq!(fake.requests(2)[1].model, "fake-model");
}

#[test]
fn init_edit_model_change_on_an_unreachable_ollama_names_how_it_is_started() {
    let home = fresh_home();
    let ollama = "default_profile = \"default\"\n\n[providers.ollama]\nkind = \"openai-compatible\"\nbase_url = \"http://127.0.0.1:1/v1\"\ntimeout_ms = 120000\n\n[profiles.default]\nprovider = \"ollama\"\nmodel = \"llama3\"\n";
    fs::write(home.join("config.toml"), ollama).unwrap();
    let transcript = succeeded(&interactive(&home, "init", "3\n1\nqwen3\ny\ny\n"));
    assert_mentions(
        &transcript,
        &[
            "Cannot list models from http://127.0.0.1:1/v1 (",
            "). Ollama is usually started with `ollama serve`. Enter the identifier manually.",
        ],
    );
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert_mentions(&written, &["model = \"qwen3\""]);
}

#[test]
fn init_edit_keeps_unmanaged_values_through_every_action() {
    for (answers, changed) in EDIT_ANSWERS {
        let installed = Installed::new(None);
        succeeded(&interactive(&installed.home, "init", answers));
        let written = installed.kept(changed);
        assert_mentions(&written, &["max_output_tokens = 77", "history_days = 30"]);
    }
}

#[test]
fn init_edit_end_of_input_or_refusal_at_every_prompt_leaves_the_file_unchanged() {
    for (answers, _) in EDIT_ANSWERS {
        let mut prefix = String::new();
        for line in answers.split_inclusive('\n') {
            for input in [prefix.clone(), format!("{prefix}n\n")] {
                let installed = Installed::new(None);
                let transcript = failed(&interactive(&installed.home, "init", &input));
                assert!(transcript.ends_with(CANCELLED), "{input:?}: {transcript}");
                assert_eq!(installed.written(), installed.original);
            }
            prefix.push_str(line);
        }
    }
}

#[test]
fn init_leaves_an_invalid_existing_configuration_byte_identical() {
    let unchanged =
        "; nothing was changed; correct the file and validate it with 'ask configure check'\n";
    for contents in [&b"SECRET_SENTINEL = 1\n"[..], &b"\xffSECRET_SENTINEL"[..]] {
        let home = fresh_home();
        let path = home.join("config.toml");
        fs::write(&path, contents).unwrap();
        let transcript = failed(&interactive(&home, "init", EDIT_ANSWERS[3].0));
        let one_safe_line = transcript.ends_with(unchanged)
            && transcript.lines().count() == 1
            && !transcript.contains("SECRET_SENTINEL");
        assert!(one_safe_line, "{transcript}");
        assert_eq!(fs::read(&path).unwrap(), contents);
    }
}

/// Runs one `init_menu_process.py` edit scenario and returns the installed
/// configuration it started from.
#[cfg(unix)]
fn terminal_edit(scenario: &str, fake: Option<&FakeProvider>, with_credential: bool) -> Installed {
    let installed = Installed::new(fake);
    terminal_run(&installed.home, scenario, fake, with_credential);
    installed
}

#[cfg(unix)]
#[test]
fn terminal_init_edit_adds_a_provider_and_sets_the_default() {
    for (scenario, changed) in [
        ("edit-provider", ADDED),
        ("edit-default", "default_profile"),
    ] {
        terminal_edit(scenario, None, false).kept(changed);
    }
}

#[cfg(unix)]
#[test]
fn terminal_init_edit_adds_a_profile_and_changes_a_model_with_verification() {
    for (scenario, changed) in [("edit-profile", ADDED), ("edit-model", "fake-model")] {
        let fake = FakeProvider::sequence(vec![Scenario::Status(200, MODELS), Scenario::Stream]);
        terminal_edit(scenario, Some(&fake), true).kept(changed);
        let requests = fake.requests(2);
        assert!(
            requests
                .iter()
                .all(|request| request.authorization_is_fixture)
        );
    }
}

#[cfg(unix)]
#[test]
fn terminal_init_edit_escape_at_every_menu_leaves_the_file_unchanged() {
    for scenario in [
        "edit-escape-action",
        "edit-escape-preset",
        "edit-escape-provider",
        "edit-escape-profile",
        "edit-escape-default",
        "edit-escape-hidden",
    ] {
        terminal_edit(scenario, None, false);
    }
    let fake = FakeProvider::start(Scenario::Status(200, MODELS));
    terminal_edit("edit-escape-model", Some(&fake), true);
    assert_eq!(fake.requests(1).len(), 1);
}

#[test]
fn init_model_change_applies_to_new_threads_only() {
    let fake = FakeProvider::sequence(vec![Scenario::Stream, Scenario::Stream, Scenario::Stream]);
    let installed = Installed::new(Some(&fake));
    let home = &installed.home;
    let first = command(home, true).args(["new", "q1"]).output().unwrap();
    assert!(first.status.success(), "{}", stderr(&first));
    succeeded(&interactive(home, "init", "3\n1\nnew-model\ny\n"));
    for arguments in [["reply", "q2"], ["new", "q3"]] {
        let output = command(home, true).args(arguments).output().unwrap();
        assert!(output.status.success(), "{}", stderr(&output));
    }
    let models: Vec<String> = fake
        .requests(3)
        .into_iter()
        .map(|request| request.model)
        .collect();
    assert_eq!(models, ["fake-model", "fake-model", "new-model"]);
}

// -- helpers ----------------------------------------------------------------

/// A model list in the OpenAI shape, with a dated snapshot listed first and
/// an identifier that would clear the screen if it were printed.
const MODELS: &str = r#"{"object":"list","data":[{"id":"fake-model-2024-08-06"},{"id":"fake-model"},{"id":"other-model"},{"id":"\u001b[2Jevil"},{"id":"other-mini"}]}"#;

#[cfg(unix)]
fn terminal_run(
    home: &Path,
    scenario: &str,
    fake: Option<&FakeProvider>,
    with_credential: bool,
) -> Output {
    let mut helper = Command::new("python3");
    helper
        .arg("tests/support/init_menu_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(home)
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

/// Runs `verb` with redirected `answers` and no credential in the environment.
fn interactive(home: &Path, verb: &str, answers: &str) -> Output {
    drive(command(home, false).arg(verb), answers)
}

/// Runs `ask init` with redirected `answers` and the fixture credential set.
fn keyed(home: &Path, answers: &str) -> Output {
    drive(command(home, true).arg("init"), answers)
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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
