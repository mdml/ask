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
    assert_eq!(target.profile, "default");
    assert_eq!(target.kind, "openai-compatible");
    assert_eq!(target.base_url, "http://127.0.0.1:1234/v1");
    assert_eq!(target.api_key_env, "LOCAL_API_KEY");
    assert_eq!(target.timeout_ms, 30_000);
    assert_eq!(target.model, "fake-model");
    assert_eq!(target.system_prompt, crate::DEFAULT_SYSTEM_PROMPT);
    assert_eq!(target.max_output_tokens, None);
}

#[test]
fn profile_output_limit_resolves_and_round_trips() {
    let configured = CONFIG.replace(
        "model = \"fake-model\"",
        "model = \"fake-model\"\nmax_output_tokens = 512",
    );
    assert_eq!(resolved(&configured).max_output_tokens, Some(512));
    let rendered = validate::document(&configured).unwrap().to_toml().unwrap();
    assert!(rendered.contains("max_output_tokens = 512"));
    let rendered = validate::document(CONFIG).unwrap().to_toml().unwrap();
    assert!(!rendered.contains("max_output_tokens"));
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
        expire_history: false,
        history_days: None,
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
        expire_history: false,
        history_days: None,
        providers: BTreeMap::new(),
        profiles: BTreeMap::from([(
            "default".to_string(),
            ProfileConfig {
                provider: "missing".to_string(),
                model: "m".to_string(),
                system_prompt: None,
                max_output_tokens: None,
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

#[test]
fn history_is_kept_forever_unless_expiry_is_enabled() {
    let history = |prefix: &str| {
        validate::document(&format!("{prefix}\n{CONFIG}"))
            .unwrap()
            .history_days()
    };
    assert_eq!(history(""), None);
    assert_eq!(history("expire_history = false"), None);
    assert_eq!(history("expire_history = true"), Some(90));
    assert_eq!(history("expire_history = true\nhistory_days = 7"), Some(7));
}

#[test]
fn rendered_toml_keeps_enabled_expiry_and_omits_the_default() {
    let mut config = validate::document(CONFIG).unwrap();
    assert!(!config.to_toml().unwrap().contains("history"));
    config.expire_history = true;
    config.history_days = Some(7);
    let rendered = config.to_toml().unwrap();
    let reparsed = validate::document(&rendered).unwrap();
    assert_eq!(reparsed.history_days(), Some(7));
}
