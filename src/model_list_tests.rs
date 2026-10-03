use super::*;

fn page(shape: Shape, body: &str) -> Page {
    parse_page(shape, body.as_bytes()).unwrap()
}

fn strings(ids: &[&str]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}

#[test]
fn openai_lists_are_one_page_of_ids() {
    let body = r#"{"object":"list","data":[{"id":"a"},{"id":"b"},{"object":"model"}]}"#;
    assert_eq!(
        page(Shape::OpenAi, body),
        Page {
            ids: strings(&["a", "b"]),
            next: None
        }
    );
    assert_eq!(page(Shape::OpenAi, "{}"), Page::default());
}

#[test]
fn entries_whose_output_modalities_exclude_text_are_dropped() {
    let body = r#"{"data":[
        {"id":"image","architecture":{"output_modalities":["image"]}},
        {"id":"both","architecture":{"output_modalities":["image","text"]}},
        {"id":"unmarked","architecture":{}}
    ]}"#;
    assert_eq!(
        page(Shape::OpenAi, body).ids,
        strings(&["both", "unmarked"])
    );
}

#[test]
fn anthropic_pages_follow_last_id_only_while_more_remain() {
    let more = r#"{"data":[{"id":"c"}],"has_more":true,"last_id":"c"}"#;
    assert_eq!(page(Shape::Anthropic, more).next.as_deref(), Some("c"));
    for last in [
        r#"{"data":[],"has_more":false,"last_id":"c"}"#,
        r#"{"data":[],"has_more":true,"last_id":""}"#,
        r#"{"data":[],"has_more":true}"#,
    ] {
        assert_eq!(page(Shape::Anthropic, last).next, None, "{last}");
    }
}

#[test]
fn gemini_keeps_generate_content_models_without_the_prefix() {
    let body = r#"{"models":[
        {"name":"models/chat","supportedGenerationMethods":["countTokens","generateContent"]},
        {"name":"models/embed","supportedGenerationMethods":["embedContent"]},
        {"name":"bare","supportedGenerationMethods":["generateContent"]},
        {"name":"models/unmarked"}
    ],"nextPageToken":"token"}"#;
    assert_eq!(
        page(Shape::Gemini, body),
        Page {
            ids: strings(&["chat", "bare"]),
            next: Some("token".to_string())
        }
    );
    let last = r#"{"models":[],"nextPageToken":""}"#;
    assert_eq!(page(Shape::Gemini, last).next, None);
}

#[test]
fn invalid_json_is_reported_without_repeating_the_body() {
    let error = parse_page(Shape::OpenAi, b"SECRET_SENTINEL").unwrap_err();
    assert_eq!(error, "the model list response is not valid JSON");
}

#[test]
fn unsafe_identifiers_are_dropped() {
    let long = "x".repeat(MAX_IDENTIFIER_BYTES + 1);
    let edge = "y".repeat(MAX_IDENTIFIER_BYTES);
    let reordered = "model\u{202e}txt";
    let hidden = "mo\u{200b}del";
    let ids = strings(&[
        "",
        "\u{1b}[2J",
        "tab\tid",
        "new\nline",
        reordered,
        hidden,
        &long,
        &edge,
        "ok",
    ]);
    assert_eq!(arrange(ids), vec![edge, "ok".to_string()]);
}

#[test]
fn duplicates_are_dropped_and_the_count_is_bounded() {
    let ids: Vec<String> = (0..MAX_ENTRIES + 5)
        .map(|index| format!("m{index}"))
        .chain(["m0".to_string()])
        .collect();
    let kept = arrange(ids);
    assert_eq!(kept.len(), MAX_ENTRIES);
    assert_eq!(arrange(strings(&["a", "a", "b"])), strings(&["a", "b"]));
}

#[test]
fn dated_snapshots_follow_the_others_in_provider_order() {
    let ids = strings(&[
        "gpt-4o-2024-08-06",
        "zeta",
        "claude-x-20241022",
        "alpha",
        "flash-preview-05-20",
        "gpt-4-0613",
    ]);
    assert_eq!(
        arrange(ids),
        strings(&[
            "zeta",
            "alpha",
            "gpt-4o-2024-08-06",
            "claude-x-20241022",
            "flash-preview-05-20",
            "gpt-4-0613",
        ])
    );
}

#[test]
fn only_date_like_suffixes_are_snapshots() {
    for id in ["m-20250101", "m-2025-12-31", "m-1231", "m-01-01"] {
        assert!(snapshot(id), "{id}");
    }
    for id in [
        "m",
        "m-001",
        "m-2507",
        "m-19991231",
        "m-20251301",
        "m-13-01",
        "m-01-32",
        "m-3-5",
        "m-00-10",
        "m-preview",
        "m-2025a101",
    ] {
        assert!(!snapshot(id), "{id}");
    }
}
