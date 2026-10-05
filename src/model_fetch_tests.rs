use super::*;

fn listing(kind: Kind, base_url: &str) -> Listing<'_> {
    Listing {
        kind,
        base_url,
        credential: Some("k ey/+"),
    }
}

#[test]
fn bearer_kinds_list_models_under_the_base_url() {
    for kind in [Kind::OpenAi, Kind::OpenRouter, Kind::OpenAiCompatible] {
        let request = listing(kind, "https://example.test/v1/").request(Some("ignored"));
        assert_eq!(
            request,
            PageRequest {
                url: "https://example.test/v1/models".to_string(),
                headers: vec![("authorization", "Bearer k ey/+".to_string())],
            }
        );
        assert_eq!(listing(kind, "").shape(), Shape::OpenAi);
    }
}

#[test]
fn anthropic_lists_with_its_key_header_and_after_id_cursor() {
    let anthropic = listing(Kind::Anthropic, "https://example.test");
    let first = anthropic.request(None);
    assert_eq!(first.url, "https://example.test/v1/models");
    assert_eq!(
        first.headers,
        vec![
            ("x-api-key", "k ey/+".to_string()),
            ("anthropic-version", "2023-06-01".to_string()),
        ]
    );
    let next = anthropic.request(Some("id&x=1"));
    assert_eq!(
        next.url,
        "https://example.test/v1/models?after_id=id%26x%3D1"
    );
    assert_eq!(anthropic.shape(), Shape::Anthropic);
}

#[test]
fn gemini_lists_with_the_key_as_an_encoded_query_parameter() {
    let gemini = listing(Kind::Gemini, "https://example.test/");
    let first = gemini.request(None);
    assert_eq!(
        first.url,
        "https://example.test/v1beta/models?pageSize=1000&key=k%20ey%2F%2B"
    );
    assert!(first.headers.is_empty());
    let next = gemini.request(Some("t/1"));
    assert_eq!(
        next.url,
        "https://example.test/v1beta/models?pageSize=1000&pageToken=t%2F1&key=k%20ey%2F%2B"
    );
    assert_eq!(gemini.shape(), Shape::Gemini);
}

#[tokio::test(flavor = "current_thread")]
async fn transport_failures_are_reported_without_the_credential() {
    let gemini = Listing {
        kind: Kind::Gemini,
        base_url: "http://127.0.0.1:1",
        credential: Some("secret-in-url"),
    };
    let error = gemini.fetch().await.unwrap_err();
    assert!(!error.contains("secret-in-url"), "{error}");
}

#[test]
fn a_keyless_listing_sends_the_placeholder_and_never_redacts_it() {
    let keyless = Listing {
        kind: Kind::OpenAiCompatible,
        base_url: "http://127.0.0.1:1/v1",
        credential: None,
    };
    assert_eq!(
        keyless.request(None).headers,
        vec![("authorization", "Bearer no-key".to_string())]
    );
    assert_eq!(keyless.redact("sent Bearer no-key"), "sent Bearer no-key");
}
