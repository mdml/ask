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

#[test]
fn parses_and_resolves_defaults() {
    let target = parse(CONFIG).unwrap().resolve().unwrap();
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
    let target = parse(&configured).unwrap().resolve().unwrap();
    assert_eq!(target.timeout_ms, 41);
    assert_eq!(target.system_prompt, "Custom");
}

#[test]
fn malformed_toml_has_a_parse_error() {
    assert!(parse("default_profile = [").is_err());
}

#[test]
fn missing_default_profile_is_named() {
    let config = CONFIG.replace(
        "default_profile = \"default\"",
        "default_profile = \"missing\"",
    );
    assert_eq!(
        config_error(&config),
        "default profile 'missing' is not configured"
    );
}

#[test]
fn missing_provider_is_named() {
    let config = CONFIG.replace("provider = \"local\"", "provider = \"missing\"");
    assert_eq!(
        config_error(&config),
        "profile 'default' references unknown provider 'missing'"
    );
}

#[test]
fn unsupported_provider_kind_is_named() {
    let config = CONFIG.replace("openai-compatible", "other");
    assert_eq!(
        config_error(&config),
        "provider 'local' has unsupported kind 'other'"
    );
}

fn config_error(contents: &str) -> String {
    parse(contents).unwrap().resolve().unwrap_err().to_string()
}

#[test]
fn rendered_toml_round_trips_and_omits_defaults() {
    let mut config = parse(CONFIG).unwrap();
    config.profiles.get_mut("default").unwrap().system_prompt =
        Some("Say \"yes\"\twith tabs".to_string());
    let rendered = config.to_toml().unwrap();
    assert!(!rendered.contains("timeout_ms"));
    let target = parse(&rendered).unwrap().resolve().unwrap();
    assert_eq!(target.system_prompt, "Say \"yes\"\twith tabs");
    assert_eq!(target.base_url, "http://127.0.0.1:1234/v1");
}

#[test]
fn rendered_toml_keeps_a_custom_timeout() {
    let mut config = parse(CONFIG).unwrap();
    config.providers.get_mut("local").unwrap().timeout_ms = 41;
    let rendered = config.to_toml().unwrap();
    assert!(rendered.contains("timeout_ms = 41"));
}
