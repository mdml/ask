//! End-to-end proof of the published model list in `ask init`, through the
//! real binary against a fake provider that serves the published document.
//! No case contacts the network.

mod support;

use std::{
    fs,
    io::{self, Write},
    path::Path,
    process::{Child, Command, Output, Stdio},
};

use support::{
    CREDENTIAL, LOOPBACK_HOSTS, MODEL_LIST_URL, PRESET_VARIABLES, PROXY_VARIABLES, command,
    fake_provider::{FakeProvider, PUBLISHED, RecordedRequest, Scenario},
    fresh_home, write_limited,
};

#[cfg(unix)]
#[test]
fn terminal_init_without_a_key_filters_the_published_list_without_verification() {
    let fake = FakeProvider::start(Scenario::Status(200, PUBLISHED));
    menu_helper("published", &[fake.base_url(), fake.published_url()], false);
    let requests = fake.requests(1);
    assert_eq!(requests.len(), 1);
    assert_published_request(&requests[0]);
    assert_eq!(fake.connections(), 1, "no verification request");
}

/// Asserts a published-list request carries no credential of any kind.
fn assert_published_request(request: &RecordedRequest) {
    assert_eq!(
        (request.method.as_str(), request.path.as_str()),
        ("GET", "/models/v1/models.json")
    );
    for name in [
        "authorization",
        "x-api-key",
        "x-goog-api-key",
        "proxy-authorization",
    ] {
        assert_eq!(request.header(name), None, "{name}");
    }
    assert!(!request.path.contains('?'), "{}", request.path);
    for (name, value) in &request.headers {
        assert!(!value.contains(CREDENTIAL), "{name}");
    }
}

/// Runs `ask init` with redirected `answers`, no credential for the chosen
/// preset, credentials for every other preset, and the published list at `fake`.
fn unkeyed_with_list(home: &Path, fake: &FakeProvider, answers: &str) -> Output {
    let mut init = command(home, false);
    for variable in PRESET_VARIABLES {
        init.env(variable, CREDENTIAL);
    }
    init.env_remove("OPENAI_API_KEY")
        .env_remove("XAI_API_KEY")
        .env(MODEL_LIST_URL, fake.published_url());
    drive(init.arg("init"), answers)
}

#[test]
fn init_without_a_key_offers_the_published_list_and_sends_no_credential() {
    let fake = FakeProvider::start(Scenario::Status(200, PUBLISHED));
    let home = fresh_home();
    let url = fake.published_url();
    let answers = "1\n0\n2\n\n\nn\ny\n";
    let transcript = succeeded(&unkeyed_with_list(&home, &fake, answers));
    for expected in [
        "OPENAI_API_KEY is not set; continuing without a key, so the setup will not be verified.",
        &format!("Requesting the published model list from {url}; no credentials are sent.\n"),
        "Published model list generated 2026-10-01; any identifier can still be entered.\n",
        " 1. fake-model\n 2. other-model\n 3. other-mini\n 4. fake-model-2024-08-06\n 5. Enter a model identifier manually\n",
        "Enter a number from 1 to 5.",
    ] {
        assert!(transcript.contains(expected), "{expected}: {transcript}");
    }
    assert!(!transcript.contains('\u{1b}'), "{transcript}");
    assert!(!transcript.contains("live check"), "{transcript}");
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("model = \"other-model\""), "{written}");
    let requests = fake.requests(1);
    assert_eq!(requests.len(), 1);
    assert_published_request(&requests[0]);
    assert_eq!(fake.connections(), 1, "no verification request");
}

#[test]
fn init_falls_back_to_manual_entry_when_the_published_list_is_unusable() {
    let cases = [
        (
            Scenario::Redirect {
                status: 302,
                port: None,
            },
            "1",
            "HTTP status 302 Found; redirects are not followed",
        ),
        (
            Scenario::OversizedList,
            "1",
            "the response is larger than 1 MiB",
        ),
        (
            Scenario::Status(200, "{not json"),
            "1",
            "the response is not valid JSON",
        ),
        (
            Scenario::Status(
                200,
                r#"{"version": 2, "generated_at": "2026-10-01T06:00:00Z", "providers": {"openai": ["m"]}}"#,
            ),
            "1",
            "the list has an unsupported format or version",
        ),
        (
            Scenario::Status(200, PUBLISHED),
            "7",
            "the list has no models for xai",
        ),
        (
            Scenario::Status(404, "{}"),
            "1",
            "HTTP status 404 Not Found",
        ),
    ];
    for case in cases {
        assert_manual_fallback(case);
    }
}

#[test]
fn init_falls_back_to_manual_entry_when_the_published_list_times_out() {
    assert_manual_fallback((
        Scenario::Silent,
        "1",
        "no complete response within 5 seconds",
    ));
}

/// Asserts that the published list served by `scenario` falls back to manual
/// entry with `reason` after choosing preset `choice`.
fn assert_manual_fallback((scenario, choice, reason): (Scenario, &str, &str)) {
    let fake = FakeProvider::start(scenario);
    let home = fresh_home();
    let answers = format!("{choice}\ntyped-model\n\n\nn\ny\n");
    let transcript = succeeded(&unkeyed_with_list(&home, &fake, &answers));
    let expected = format!(
        "\nCannot use the published model list ({reason}); enter the identifier manually.\nModel identifier (free text sent to the provider): "
    );
    assert!(transcript.contains(&expected), "{reason}: {transcript}");
    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(written.contains("model = \"typed-model\""), "{written}");
    assert_eq!(
        fake.connections(),
        1,
        "{reason}: redirects are not followed"
    );
    assert_published_request(&fake.requests(1)[0]);
}

#[test]
fn init_with_a_key_uses_the_provider_list_and_not_the_published_list() {
    // A loopback proxy stands in for the preset's endpoint, so the CONNECT
    // requests it records show where each request went without leaving the host.
    let fake = FakeProvider::start(Scenario::Status(502, "{}"));
    let home = fresh_home();
    let mut init = command(&home, false);
    for variable in [
        "http_proxy",
        "HTTP_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "no_proxy",
    ] {
        init.env_remove(variable);
    }
    init.env("OPENAI_API_KEY", CREDENTIAL)
        .env("HTTPS_PROXY", format!("http://{}", fake.address()))
        .env("NO_PROXY", "127.0.0.1")
        .env(MODEL_LIST_URL, fake.published_url());
    let answers = "1\ntyped-model\n\n\ny\nn\ny\n";
    let transcript = succeeded(&drive(init.arg("init"), answers));
    assert!(transcript.contains("Requesting the model list from https://api.openai.com/v1."));
    assert!(!transcript.contains("published model list"), "{transcript}");
    assert!(!transcript.contains(CREDENTIAL));
    let requests = fake.requests(2);
    assert_eq!(fake.connections(), 2, "the list and the verification");
    for request in &requests {
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("CONNECT", "api.openai.com:443")
        );
    }
}

#[test]
fn custom_and_keyless_providers_never_request_the_published_list() {
    // A keyless endpoint is also verified; the unreachable one fails and is written anyway.
    for (variable, rest) in [("LOCAL_API_KEY", "n\ny\n"), ("", "y\nn\ny\n")] {
        let fake = FakeProvider::start(Scenario::Status(200, PUBLISHED));
        let home = fresh_home();
        let answers = format!("9\nlocal\nhttp://127.0.0.1:1/v1\n{variable}\nm\n\n\n{rest}");
        let mut init = command(&home, false);
        init.env(MODEL_LIST_URL, fake.published_url());
        let transcript = succeeded(&drive(init.arg("init"), &answers));
        assert!(!transcript.contains("published model list"), "{transcript}");
        assert_eq!(fake.connections(), 0);
    }
}

#[test]
fn every_process_the_proofs_start_disables_the_published_list() {
    let home = fresh_home();
    let ask = command(&home, false);
    let set: Vec<_> = ask
        .get_envs()
        .filter(|(name, _)| *name == MODEL_LIST_URL)
        .collect();
    assert_eq!(set, [(MODEL_LIST_URL.as_ref(), Some("".as_ref()))]);
    let limited = write_limited(&home, &["init"]);
    assert!(
        limited
            .get_envs()
            .any(|entry| entry == (MODEL_LIST_URL.as_ref(), Some("".as_ref())))
    );
    for launcher in python_launchers() {
        let source = fs::read_to_string(&launcher).unwrap();
        assert!(
            source.contains("'ASK_MODEL_LIST_URL'] = ")
                || source.contains("\"ASK_MODEL_LIST_URL\": \"\""),
            "{} starts ask without setting ASK_MODEL_LIST_URL",
            launcher.display()
        );
    }
}

#[test]
fn every_process_the_proofs_start_ignores_ambient_proxies() {
    let home = fresh_home();
    for launched in [command(&home, false), write_limited(&home, &["init"])] {
        let mut proxies: Vec<_> = launched
            .get_envs()
            .filter(|(name, _)| PROXY_VARIABLES.iter().any(|variable| name == variable))
            .map(|(name, value)| {
                (
                    name.to_str().unwrap(),
                    value.and_then(|value| value.to_str()),
                )
            })
            .collect();
        proxies.sort_unstable();
        let mut expected: Vec<_> = PROXY_VARIABLES
            .map(|name| (name, (name == "NO_PROXY").then_some(LOOPBACK_HOSTS)))
            .into();
        expected.sort_unstable();
        assert_eq!(proxies, expected);
    }
    // The support helpers clear proxies from their own environment, which ask
    // inherits; the acceptance and demo scripts give ask a fresh environment.
    for launcher in python_launchers() {
        let source = fs::read_to_string(&launcher).unwrap();
        assert!(
            source.contains(PYTHON_PROXY_CLEARING)
                || source.contains("\"NO_PROXY\": \"127.0.0.1\""),
            "{} starts ask without clearing proxy variables",
            launcher.display()
        );
    }
}

// -- helpers ----------------------------------------------------------------

/// The lines with which each support helper clears proxy variables.
const PYTHON_PROXY_CLEARING: &str =
    "for variable in ('HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY'):
    os.environ.pop(variable, None)
    os.environ.pop(variable.lower(), None)
os.environ['NO_PROXY'] = '127.0.0.1,localhost'
";

/// Every Python script that starts the binary.
fn python_launchers() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut launchers: Vec<_> = fs::read_dir(root.join("tests/support"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "py"))
        .collect();
    launchers
        .extend(["scripts/offline-acceptance.py", "scripts/demo.py"].map(|path| root.join(path)));
    launchers
}

/// Runs one `init_menu_process.py` scenario with extra `arguments`: the
/// fake's base URL and, when present, its published-list URL.
#[cfg(unix)]
fn menu_helper(scenario: &str, arguments: &[String], with_credential: bool) -> Output {
    let home = fresh_home();
    let mut helper = Command::new("python3");
    helper
        .arg("tests/support/init_menu_process.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .arg(&home)
        .arg(scenario)
        .args(arguments)
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
