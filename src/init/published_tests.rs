use super::*;

const DOCUMENT: &str = r#"{"version": 1, "generated_at": "2026-10-01T06:00:00Z", "providers": {
    "openai": ["model-2024-08-06", "model", "", "model", "\u001b[2Jevil", 7, "mini"],
    "groq": []
}}"#;

#[test]
fn a_version_one_document_lists_sanitized_identifiers_in_order() {
    let published = parse(DOCUMENT.as_bytes(), "openai").unwrap();
    assert_eq!(
        published,
        Published {
            ids: vec![
                "model".to_string(),
                "mini".to_string(),
                "model-2024-08-06".to_string()
            ],
            generated: "2026-10-01".to_string(),
        }
    );
}

#[test]
fn documents_outside_the_format_are_refused_with_a_reason() {
    for (body, provider, reason) in [
        ("not json", "openai", "the response is not valid JSON"),
        (
            "[1]",
            "openai",
            "the list has an unsupported format or version",
        ),
        (
            r#"{"version": 2, "generated_at": "2026-10-01T06:00:00Z", "providers": {"openai": ["m"]}}"#,
            "openai",
            "the list has an unsupported format or version",
        ),
        (
            r#"{"version": 1, "providers": {"openai": ["m"]}}"#,
            "openai",
            "the list has no valid generated_at time",
        ),
        (DOCUMENT, "groq", "the list has no models for groq"),
        (DOCUMENT, "xai", "the list has no models for xai"),
        (
            r#"{"version": 1, "generated_at": "2026-10-01T06:00:00Z", "providers": []}"#,
            "openai",
            "the list has no models for openai",
        ),
    ] {
        assert_eq!(
            parse(body.as_bytes(), provider).unwrap_err(),
            reason,
            "{body}"
        );
    }
}

#[test]
fn only_the_date_of_an_rfc_3339_time_is_shown() {
    for (value, expected) in [
        (Value::from("2026-10-01T06:00:00Z"), Some("2026-10-01")),
        (
            Value::from("2026-10-01t06:00:00.5+02:00"),
            Some("2026-10-01"),
        ),
        (Value::from("2026-10-01"), None),
        (Value::from("2026-10-01 06:00:00Z"), None),
        (Value::from("\u{1b}[2J-10-01T06:00:00Z"), None),
        (Value::from("2026/10/01T06:00:00Z"), None),
        (Value::from("é026-10-01T"), None),
        (Value::from(20_261_001), None),
    ] {
        assert_eq!(generated_date(&value).as_deref(), expected, "{value}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn transport_failures_are_reported() {
    let error = fetch("http://127.0.0.1:1/v1/models.json", "openai")
        .await
        .unwrap_err();
    assert!(!error.is_empty());
    let error = fetch("not a url", "openai").await.unwrap_err();
    assert!(!error.is_empty());
}
