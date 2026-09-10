use super::*;

const CONFIG: &str = r#"
default_profile = "default"

[providers.local]
kind = "openai-compatible"
base_url = "http://127.0.0.1:1234/v1"
api_key_env = "LOCAL_API_KEY"

[profiles.default]
provider = "local"
model = "fake-model"
"#;

fn resolved(contents: &str) -> Target {
    validate::document(contents).unwrap().resolve().unwrap()
}

#[test]
fn parses_and_resolves_defaults() {
    let target = resolved(CONFIG);
    assert_eq!(target.base_url, "http://127.0.0.1:1234/v1");
    assert_eq!(target.api_key_env, "LOCAL_API_KEY");
    assert_eq!(target.timeout_ms, 30_000);
    assert_eq!(target.model, "fake-model");
    assert_eq!(target.system_prompt, crate::DEFAULT_SYSTEM_PROMPT);
}

#[test]
fn profile_replaces_prompt_and_provider_timeout() {
    let configured = CONFIG
        .replace(
            "api_key_env = \"LOCAL_API_KEY\"",
            "api_key_env = \"LOCAL_API_KEY\"\ntimeout_ms = 41",
        )
        .replace(
            "model = \"fake-model\"",
            "model = \"fake-model\"\nsystem_prompt = \"Custom\"",
        );
    let target = resolved(&configured);
    assert_eq!(target.timeout_ms, 41);
    assert_eq!(target.system_prompt, "Custom");
}

#[test]
fn resolve_reports_a_default_profile_that_is_absent() {
    let config = Config {
        default_profile: "missing".to_string(),
        providers: BTreeMap::new(),
        profiles: BTreeMap::new(),
    };
    assert_eq!(
        config.resolve().unwrap_err().to_string(),
        "default_profile references an unknown profile"
    );
}

#[test]
fn resolve_reports_a_provider_that_is_absent() {
    let config = Config {
        default_profile: "default".to_string(),
        providers: BTreeMap::new(),
        profiles: BTreeMap::from([(
            "default".to_string(),
            ProfileConfig {
                provider: "missing".to_string(),
                model: "m".to_string(),
                system_prompt: None,
            },
        )]),
    };
    assert_eq!(
        config.resolve().unwrap_err().to_string(),
        "profiles.provider references an unknown provider"
    );
}

#[test]
fn rendered_toml_round_trips_and_omits_defaults() {
    let mut config = validate::document(CONFIG).unwrap();
    config.profiles.get_mut("default").unwrap().system_prompt =
        Some("Say \"yes\"\twith tabs".to_string());
    let rendered = config.to_toml().unwrap();
    assert!(!rendered.contains("timeout_ms"));
    let target = resolved(&rendered);
    assert_eq!(target.system_prompt, "Say \"yes\"\twith tabs");
    assert_eq!(target.base_url, "http://127.0.0.1:1234/v1");
}

#[test]
fn rendered_toml_keeps_a_custom_timeout() {
    let mut config = validate::document(CONFIG).unwrap();
    config.providers.get_mut("local").unwrap().timeout_ms = 41;
    let rendered = config.to_toml().unwrap();
    assert!(rendered.contains("timeout_ms = 41"));
}
