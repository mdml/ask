use super::*;
use crate::store::TargetHealth;

#[test]
fn valid_configuration_reports_environment_exit_for_missing_credential() {
    let mut diagnosis = Diagnosis::new();
    diagnosis.finish(FinishInput {
        config: Some(sample_config()),
        paths: sample_paths(),
        database: DatabaseState::Absent,
        targets: vec![TargetReport {
            target: sample_target(),
            credential: CredentialState::Missing,
            history: None,
            live: None,
        }],
    });
    diagnosis.environment_failed = true;
    let text = diagnosis.render();
    assert!(text.contains("configuration: valid"));
    assert!(text.contains("credential LOCAL_API_KEY: missing"));
    assert_eq!(diagnosis.status(), exit_environment());
}

#[test]
fn configuration_failure_uses_exit_one() {
    let mut diagnosis = Diagnosis::new();
    diagnosis.note_config("invalid configuration".to_string());
    diagnosis.finish(FinishInput {
        config: None,
        paths: sample_paths(),
        database: DatabaseState::Absent,
        targets: vec![],
    });
    assert_eq!(diagnosis.status(), exit_config());
    assert!(diagnosis.render().contains("configuration: invalid"));
}

#[test]
fn limited_database_state_uses_environment_exit() {
    let mut diagnosis = Diagnosis::new();
    diagnosis.finish(FinishInput {
        config: Some(sample_config()),
        paths: sample_paths(),
        database: DatabaseState::Limited("companion file present".into()),
        targets: vec![],
    });
    diagnosis.environment_failed = true;
    assert_eq!(diagnosis.status(), exit_environment());
    assert!(diagnosis.render().contains("limited check:"));
}

#[test]
fn historical_health_renders_last_observed_wording_and_source() {
    let line = render_health(&TargetHealth {
        kind: "openai-compatible".to_string(),
        base_url: "http://127.0.0.1/v1".to_string(),
        model: "fake-model".to_string(),
        queries: 1,
        last_success_at_ms: Some(0),
        last_success_source: Some("query".to_string()),
        last_failure_at_ms: None,
        last_failure_class: None,
        last_failure_source: None,
    });
    assert!(line.contains("last observed healthy 1970-01-01 00:00 UTC (from query)"));
    assert!(line.contains("no failures observed"));
}

fn sample_config() -> Config {
    crate::validate::document(
        r#"
default_profile = "default"

[providers.local]
kind = "openai-compatible"
base_url = "http://127.0.0.1:1/v1"
api_key_env = "LOCAL_API_KEY"

[profiles.default]
provider = "local"
model = "fake-model"
"#,
    )
    .unwrap()
}

fn sample_target() -> Target {
    sample_config().resolve_named("default").unwrap()
}

fn sample_paths() -> Paths {
    Paths {
        ask_home: Some(PathBuf::from("/tmp/ask-home")),
        config: PathBuf::from("/tmp/ask-home/config.toml"),
        data: PathBuf::from("/tmp/ask-home/data/ask.sqlite3"),
        cache: PathBuf::from("/tmp/ask-home/cache"),
    }
}
