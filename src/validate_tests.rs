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

fn problem(contents: &str) -> String {
    document(contents).unwrap_err()
}

#[test]
fn a_complete_document_is_accepted() {
    assert!(document(CONFIG).is_ok());
}

#[test]
fn malformed_toml_is_located_without_quoting_the_document() {
    let message = problem("default_profile = [\n");
    assert!(message.contains("(line 1, column 20)"), "{message}");
    assert!(!message.contains("default_profile = ["), "{message}");
}

#[test]
fn an_unknown_top_level_field_is_named() {
    let message = problem(&format!("retention_days = 7\n{CONFIG}"));
    assert!(
        message.starts_with("configuration: unknown field at key index "),
        "{message}"
    );
}

#[test]
fn unknown_entry_fields_are_located_by_schema_and_index() {
    for (table, field) in [("providers", "api_key_env"), ("profiles", "model")] {
        let candidate = CONFIG.replace(&format!("{field} ="), "SECRET_SENTINEL =");
        let message = problem(&candidate);
        assert!(
            message.starts_with(&format!("{table}[1]: unknown field at key index ")),
            "{message}"
        );
        assert!(!message.contains("SECRET_SENTINEL"));
    }
}

#[test]
fn a_missing_required_field_is_named() {
    let message = problem(&CONFIG.replace("model = \"fake-model\"\n", ""));
    assert!(
        message.starts_with("profiles[1].model: missing required field"),
        "{message}"
    );
}

#[test]
fn a_wrong_type_is_reported_without_its_value() {
    let message = problem(&CONFIG.replace(
        "api_key_env = \"LOCAL_API_KEY\"",
        "api_key_env = \"LOCAL_API_KEY\"\ntimeout_ms = \"soon-enough\"",
    ));
    assert!(
        message.starts_with("providers[1].timeout_ms: invalid type or range"),
        "{message}"
    );
    assert!(!message.contains("soon-enough"), "{message}");
}

#[test]
fn every_provider_is_validated_not_only_the_selected_one() {
    let unused = format!(
        "{CONFIG}\n[providers.other]\nkind = \"proprietary\"\nbase_url = \"http://x.test/v1\"\napi_key_env = \"K\"\n"
    );
    assert_eq!(
        problem(&unused),
        "providers[2].kind has an unsupported kind; the only supported kind is 'openai-compatible'"
    );
}

#[test]
fn every_profile_reference_is_validated_not_only_the_default_one() {
    let unused =
        format!("{CONFIG}\n[profiles.other]\nprovider = \"missing\"\nmodel = \"fake-model\"\n");
    assert_eq!(
        problem(&unused),
        "profiles[2].provider references an unknown provider"
    );
}

#[test]
fn a_default_profile_that_is_absent_is_named() {
    let message = problem(&CONFIG.replace(
        "default_profile = \"default\"",
        "default_profile = \"missing\"",
    ));
    assert_eq!(message, "default_profile references an unknown profile");
}

#[test]
fn provider_values_are_validated() {
    let cases = [
        (
            "base_url = \"http://127.0.0.1:1234/v1\"",
            "base_url = \"ftp://example.test/v1\"",
            format!("providers[1].base_url {ENDPOINT_RULE}"),
        ),
        (
            "api_key_env = \"LOCAL_API_KEY\"",
            "api_key_env = \"1KEY\"",
            format!("providers[1].api_key_env {ENV_VAR_RULE}"),
        ),
        (
            "api_key_env = \"LOCAL_API_KEY\"",
            "api_key_env = \"LOCAL_API_KEY\"\ntimeout_ms = 0",
            "providers[1].timeout_ms must be greater than zero".to_string(),
        ),
    ];
    for (from, to, expected) in cases {
        assert_eq!(problem(&CONFIG.replace(from, to)), expected);
    }
}

#[test]
fn profile_values_are_validated() {
    let cases = [(
        "model = \"fake-model\"",
        "model = \"\"",
        format!("profiles[1].model {REQUIRED_RULE}"),
    )];
    for (from, to, expected) in cases {
        assert_eq!(problem(&CONFIG.replace(from, to)), expected);
    }
}

#[test]
fn empty_provider_and_profile_names_are_rejected() {
    assert_eq!(
        problem(&CONFIG.replace("[providers.local]", "[providers.\"\"]")),
        format!("providers[1].name {REQUIRED_RULE}")
    );
    assert_eq!(
        problem(&CONFIG.replace("[profiles.default]", "[profiles.\"\"]")),
        format!("profiles[1].name {REQUIRED_RULE}")
    );
}

#[test]
fn endpoints_are_accepted_and_rejected_by_rule() {
    for accepted in [
        "https://a",
        "http://[::1]:8080/v1",
        "https://example.test/v1",
        "https://example.test/v1%23fragment",
    ] {
        assert!(endpoint(Value(accepted)).is_ok(), "{accepted}");
    }
    for rejected in [
        "https://",
        "https:///v1",
        "https://?q=x",
        "https://user:secret@example.test/v1",
        "https://example.test/v1?key=value",
        "https://example.test/v1?",
        "https://example.test/v1#fragment",
        "https://example.test/v1#",
        "not-a-url",
    ] {
        assert!(endpoint(Value(rejected)).is_err(), "{rejected}");
    }
}

#[test]
fn environment_variable_names_are_accepted_and_rejected_by_rule() {
    assert!(env_var_name(Value("_A1")).is_ok());
    for rejected in ["", "A B", "1KEY", "BAD-NAME"] {
        assert!(env_var_name(Value(rejected)).is_err(), "{rejected}");
    }
}

#[test]
fn positions_count_lines_and_columns_from_one() {
    assert_eq!(Document("abc").position(0), (1, 1));
    assert_eq!(Document("ab\ncd").position(4), (2, 2));
    assert_eq!(Document("ab\ncd").position(99), (2, 3));
}
