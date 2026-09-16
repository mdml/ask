//! Per-provider wire-format proofs: each supported `kind` sends its own
//! request shape, streams text and usage, carries reply history, honors the
//! output-token policy, and fails safely, all against the loopback fake.

mod support;

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::Output,
};

use rig_core::serde_json::{Value, json};
use support::{
    CREDENTIAL, command,
    fake_provider::{FakeProvider, RecordedRequest, Scenario},
    fresh_home,
};

const OPENAI_STREAM: &str = concat!(
    "data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"**\",\"sequence_number\":1}\n\n",
    "data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"4**\",\"sequence_number\":2}\n\n",
    "data: {\"type\":\"response.completed\",\"sequence_number\":3,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"error\":null,\"incomplete_details\":null,\"instructions\":null,\"max_output_tokens\":null,\"model\":\"m\",\"usage\":{\"input_tokens\":12,\"output_tokens\":3,\"total_tokens\":15},\"output\":[],\"tools\":[]}}\n\n",
);
const OPENAI_TRUNCATED: &str = concat!(
    "data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"**4**\",\"sequence_number\":1}\n\n",
    "data: {\"type\":\"response.incomplete\",\"sequence_number\":2,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"incomplete\",\"error\":null,\"incomplete_details\":{\"reason\":\"max_output_tokens\"},\"instructions\":null,\"max_output_tokens\":3,\"model\":\"m\",\"usage\":{\"input_tokens\":12,\"output_tokens\":3,\"total_tokens\":15},\"output\":[],\"tools\":[]}}\n\n",
);
const ANTHROPIC_STREAM: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"m\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"**\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"4**\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":3}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);
const ANTHROPIC_TRUNCATED: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"m\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"**4**\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":3}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);
const GEMINI_STREAM: &str = concat!(
    "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"**\"}],\"role\":\"model\"},\"index\":0}]}\n\n",
    "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"4**\"}],\"role\":\"model\"},\"finishReason\":\"STOP\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":3,\"totalTokenCount\":15}}\n\n",
);
const GEMINI_TRUNCATED: &str = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"**4**\"}],\"role\":\"model\"},\"finishReason\":\"MAX_TOKENS\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":3,\"totalTokenCount\":15}}\n\n";
const OPENAI_REFUSAL: [&str; 2] = [
    "data: {\"type\":\"response.incomplete\",\"sequence_number\":2,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"incomplete\",\"error\":null,\"incomplete_details\":{\"reason\":\"content_filter\"},\"instructions\":null,\"max_output_tokens\":null,\"model\":\"m\",\"usage\":{\"input_tokens\":12,\"output_tokens\":3,\"total_tokens\":15},\"output\":[],\"tools\":[]}}\n\n",
    concat!(
        "data: {\"type\":\"response.refusal.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"cannot comply\",\"sequence_number\":1}\n\n",
        "data: {\"type\":\"response.incomplete\",\"sequence_number\":2,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"incomplete\",\"error\":null,\"incomplete_details\":{\"reason\":\"content_filter\"},\"instructions\":null,\"max_output_tokens\":null,\"model\":\"m\",\"usage\":{\"input_tokens\":12,\"output_tokens\":3,\"total_tokens\":15},\"output\":[],\"tools\":[]}}\n\n",
    ),
];
const ANTHROPIC_REFUSAL: [&str; 2] = [
    concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"m\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":3}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ),
    concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"m\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"cannot comply\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":3}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ),
];
const GEMINI_REFUSAL: [&str; 2] = [
    "data: {\"candidates\":[{\"content\":{\"parts\":[],\"role\":\"model\"},\"finishReason\":\"SAFETY\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":3,\"totalTokenCount\":15}}\n\n",
    "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"cannot comply\"}],\"role\":\"model\"},\"finishReason\":\"SAFETY\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":3,\"totalTokenCount\":15}}\n\n",
];
const CHAT_REFUSAL: [&str; 2] = [
    concat!(
        "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"content_filter\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"total_tokens\":15}}\n\n",
        "data: [DONE]\n\n",
    ),
    concat!(
        "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":null,\"refusal\":\"cannot comply\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"content_filter\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"total_tokens\":15}}\n\n",
        "data: [DONE]\n\n",
    ),
];
const GEMINI_PROMPT_REFUSAL: &str = "data: {\"candidates\":[],\"promptFeedback\":{\"blockReason\":\"SAFETY\"},\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":0,\"totalTokenCount\":12}}\n\n";
const OPENAI_EMPTY: &str = "data: {\"type\":\"response.completed\",\"sequence_number\":1,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"error\":null,\"incomplete_details\":null,\"instructions\":null,\"max_output_tokens\":null,\"model\":\"m\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0,\"total_tokens\":12},\"output\":[],\"tools\":[]}}\n\n";
const ANTHROPIC_EMPTY: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"m\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":0}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);
const GEMINI_EMPTY: &str = "data: {\"candidates\":[{\"content\":{\"parts\":[],\"role\":\"model\"},\"finishReason\":\"STOP\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":0,\"totalTokenCount\":12}}\n\n";
const GEMINI_THOUGHT_AND_ANSWER: &str = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"hidden answer\"},{\"text\":\"visible answer\"}],\"role\":\"model\"},\"finishReason\":\"STOP\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":3,\"totalTokenCount\":15}}\n\n";
const GEMINI_LATE_USAGE: &str = concat!(
    "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"answer\"}],\"role\":\"model\"},\"finishReason\":\"STOP\",\"index\":0}]}\n\n",
    "data: {\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":3,\"totalTokenCount\":15}}\n\n",
);
const CHAT_EMPTY: &str = concat!(
    "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":0,\"total_tokens\":12}}\n\n",
    "data: [DONE]\n\n",
);
const CHAT_STREAM: &str = concat!(
    "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"**\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"4**\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"total_tokens\":15}}\n\n",
    "data: [DONE]\n\n",
);
const CHAT_TRUNCATED: &str = concat!(
    "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"**4**\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"total_tokens\":15}}\n\n",
    "data: [DONE]\n\n",
);
/// OpenRouter's documented mid-stream error chunk: HTTP 200, a top-level
/// `error`, and `finish_reason: "error"`.
const OPENROUTER_IN_BAND_ERROR: &str = concat!(
    "data: {\"id\":\"gen-1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"partial\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"gen-1\",\"model\":\"m\",\"error\":{\"code\":502,\"message\":\"upstream failed\"},\"choices\":[{\"index\":0,\"delta\":{\"content\":\"\"},\"finish_reason\":\"error\"}]}\n\n",
    "data: [DONE]\n\n",
);
const ECHOED_CREDENTIAL: &str =
    "{\"error\":{\"code\":401,\"message\":\"invalid key credential-secret-never-print\"}}";

const TRUNCATION_WARNING: &str = "ask: warning: answer stopped at the provider's output-token limit; set a larger max_output_tokens in the profile\n";

/// One supported provider kind and its wire format.
#[derive(Clone, Copy)]
struct Wire {
    kind: &'static str,
    /// Appended to the fake's origin to form the configured `base_url`.
    base_path: &'static str,
    stream: &'static str,
    truncated: &'static str,
}

const OPENAI: Wire = Wire {
    kind: "openai",
    base_path: "/v1",
    stream: OPENAI_STREAM,
    truncated: OPENAI_TRUNCATED,
};
const ANTHROPIC: Wire = Wire {
    kind: "anthropic",
    base_path: "",
    stream: ANTHROPIC_STREAM,
    truncated: ANTHROPIC_TRUNCATED,
};
const GEMINI: Wire = Wire {
    kind: "gemini",
    base_path: "",
    stream: GEMINI_STREAM,
    truncated: GEMINI_TRUNCATED,
};
const OPENROUTER: Wire = Wire {
    kind: "openrouter",
    base_path: "/api/v1",
    stream: CHAT_STREAM,
    truncated: CHAT_TRUNCATED,
};
const COMPATIBLE: Wire = Wire {
    kind: "openai-compatible",
    base_path: "/v1",
    stream: CHAT_STREAM,
    truncated: CHAT_TRUNCATED,
};

const WIRES: [Wire; 5] = [OPENAI, ANTHROPIC, GEMINI, OPENROUTER, COMPATIBLE];

#[derive(Clone, Copy)]
struct Setup {
    model: &'static str,
    profile: &'static str,
}

const STANDARD: Setup = Setup {
    model: "model",
    profile: "",
};

fn refusal(wire: Wire, with_text: bool) -> &'static str {
    let index = usize::from(with_text);
    match wire.kind {
        "openai" => OPENAI_REFUSAL[index],
        "anthropic" => ANTHROPIC_REFUSAL[index],
        "gemini" => GEMINI_REFUSAL[index],
        _ => CHAT_REFUSAL[index],
    }
}

fn empty_success(wire: Wire) -> &'static str {
    match wire.kind {
        "openai" => OPENAI_EMPTY,
        "anthropic" => ANTHROPIC_EMPTY,
        "gemini" => GEMINI_EMPTY,
        _ => CHAT_EMPTY,
    }
}

#[test]
fn every_kind_streams_an_answer_and_replays_history_in_its_wire_format() {
    for wire in WIRES {
        const MODEL: &str = "vendor/future-model:beta";
        let fake = FakeProvider::start(Scenario::Sse(wire.stream));
        let home = configured(
            &fake,
            wire,
            Setup {
                model: MODEL,
                profile: "",
            },
        );
        for args in [&["first question"][..], &["reply", "second question"]] {
            let output = ask(&home, args);
            assert!(output.status.success(), "{}: {output:?}", wire.kind);
            assert_eq!(output.stdout, b"**4**\n", "{}", wire.kind);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("12 in / 3 out"), "{}: {stderr}", wire.kind);
            assert_eq!(stderr.lines().count(), 1, "{}: {stderr}", wire.kind);
        }
        let requests = fake.requests(2);
        assert_eq!(requests.len(), 2, "{}", wire.kind);
        assert_first_request(wire, &requests[0]);
        assert_eq!(
            conversation(wire, &requests[1]),
            [
                ("system", ask::DEFAULT_SYSTEM_PROMPT),
                ("user", "first question"),
                ("assistant", "**4**"),
                ("user", "second question"),
            ]
            .map(|(role, text)| (role.to_string(), text.to_string())),
            "{}",
            wire.kind
        );
    }
}

/// Endpoint path, credential location, and the verbatim model identifier.
fn assert_first_request(wire: Wire, request: &RecordedRequest) {
    let bearer = format!("Bearer {CREDENTIAL}");
    match wire.kind {
        "openai" => assert_openai_request(request, &bearer),
        "anthropic" => assert_anthropic_request(request),
        "gemini" => assert_gemini_request(request),
        "openrouter" => assert_chat_request(request, "/api/v1/chat/completions", &bearer),
        _ => assert_chat_request(request, "/v1/chat/completions", &bearer),
    }
    if wire.kind != "anthropic" {
        assert_eq!(request.header("x-api-key"), None, "{}", wire.kind);
    }
    if matches!(wire.kind, "anthropic" | "gemini") {
        assert_eq!(request.header("authorization"), None, "{}", wire.kind);
    }
}

fn assert_openai_request(request: &RecordedRequest, bearer: &str) {
    assert_eq!(
        (
            request.path.as_str(),
            request.header("authorization"),
            &request.body["model"],
            &request.body["store"],
            &request.body["stream"],
        ),
        (
            "/v1/responses",
            Some(bearer),
            &json!("vendor/future-model:beta"),
            &json!(false),
            &json!(true),
        )
    );
}

fn assert_anthropic_request(request: &RecordedRequest) {
    assert_eq!(
        (
            request.path.as_str(),
            request.header("x-api-key"),
            request.header("anthropic-version").is_some(),
            &request.body["model"],
        ),
        (
            "/v1/messages",
            Some(CREDENTIAL),
            true,
            &json!("vendor/future-model:beta"),
        )
    );
}

fn assert_gemini_request(request: &RecordedRequest) {
    assert_eq!(
        request.path,
        format!(
            "/v1beta/models/vendor%2Ffuture-model%3Abeta:streamGenerateContent?alt=sse&key={CREDENTIAL}"
        )
    );
    assert_eq!(request.body.get("model"), None);
}

fn assert_chat_request(request: &RecordedRequest, path: &str, bearer: &str) {
    assert_eq!(
        (
            request.path.as_str(),
            request.header("authorization"),
            &request.body["model"],
        ),
        (path, Some(bearer), &json!("vendor/future-model:beta"),)
    );
}

struct ExpectedFailure<'a> {
    stdout: &'a [u8],
    stderr: &'a str,
}

fn assert_failure(output: &Output, expected: ExpectedFailure<'_>, wire: Wire) {
    assert_eq!(output.status.code(), Some(1), "{}: {output:?}", wire.kind);
    assert_eq!(output.stdout, expected.stdout, "{}", wire.kind);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        expected.stderr,
        "{}",
        wire.kind
    );
}

/// The system prompt and messages of a request as (role, text) pairs, with
/// each format's roles translated to user and assistant.
fn conversation(wire: Wire, request: &RecordedRequest) -> Vec<(String, String)> {
    let body = &request.body;
    let (system, messages, list) = match wire.kind {
        "openai" => (text(&body["instructions"]), &body["input"], "content"),
        "anthropic" => (text(&body["system"]), &body["messages"], "content"),
        "gemini" => (
            text(&body["systemInstruction"]["parts"]),
            &body["contents"],
            "parts",
        ),
        _ => return request.messages.clone(),
    };
    let mut pairs = vec![("system".to_string(), system)];
    for message in messages.as_array().unwrap() {
        let role = match message["role"].as_str().unwrap() {
            "model" => "assistant",
            other => other,
        };
        pairs.push((role.to_string(), text(&message[list])));
    }
    pairs
}

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().map(|part| text(&part["text"])).collect(),
        _ => String::new(),
    }
}

#[test]
fn output_limit_defaults_only_where_required_and_overrides_everywhere() {
    for wire in WIRES {
        for (setting, expected) in [("", None), ("max_output_tokens = 77\n", Some(77))] {
            let fake = FakeProvider::start(Scenario::Sse(wire.stream));
            let home = configured(
                &fake,
                wire,
                Setup {
                    model: "model",
                    profile: setting,
                },
            );
            assert!(ask(&home, &["question"]).status.success());
            let body = fake.recorded().unwrap().body;
            let sent = match wire.kind {
                "openai" => &body["max_output_tokens"],
                "gemini" => &body["generationConfig"]["maxOutputTokens"],
                _ => &body["max_tokens"],
            };
            let expected = match (wire.kind, expected) {
                ("anthropic", None) => json!(4096),
                (_, Some(limit)) => json!(limit),
                (_, None) => Value::Null,
            };
            assert_eq!(sent, &expected, "{} {setting}", wire.kind);
        }
    }
}

#[test]
fn a_reported_output_limit_is_a_partial_turn_excluded_from_replies() {
    for wire in WIRES {
        let fake = FakeProvider::sequence(vec![
            Scenario::Sse(wire.truncated),
            Scenario::Sse(wire.stream),
        ]);
        let home = configured(
            &fake,
            wire,
            Setup {
                model: "model",
                profile: "max_output_tokens = 3\n",
            },
        );
        let output = ask(&home, &["long question"]);
        assert_failure(
            &output,
            ExpectedFailure {
                stdout: b"**4**\n",
                stderr: TRUNCATION_WARNING,
            },
            wire,
        );
        assert_eq!(
            row(
                &home,
                "SELECT t.status || ':' || t.answer || ':' || s.outcome || ':' || s.error_class || ':' || s.output_tokens || ':' || (h.last_success_at_ms IS NOT NULL) || (h.last_failure_at_ms IS NULL) || ':' || (SELECT count(*) FROM current_thread) FROM turns t, query_statistics s, provider_health h"
            ),
            "partial:**4**:partial:output_limit:3:11:1",
            "{}",
            wire.kind
        );
        assert!(ask(&home, &["reply", "next"]).status.success());
        let reply = &fake.requests(2)[1];
        let roles: Vec<String> = conversation(wire, reply)
            .into_iter()
            .map(|(role, _)| role)
            .collect();
        assert_eq!(roles, ["system", "user"], "{}", wire.kind);
    }
}

#[test]
fn an_openrouter_in_band_stream_error_is_not_a_complete_answer() {
    let fake = FakeProvider::start(Scenario::Sse(OPENROUTER_IN_BAND_ERROR));
    let home = configured(&fake, OPENROUTER, STANDARD);
    let output = ask(&home, &["question"]);
    assert_failure(
        &output,
        ExpectedFailure {
            stdout: b"partial\n",
            stderr: "ask: provider request failed: the provider reported an error in the answer stream\n",
        },
        OPENROUTER,
    );
    assert_eq!(
        row(
            &home,
            "SELECT t.status || ':' || s.outcome || ':' || s.error_class || ':' || h.last_failure_class FROM turns t, query_statistics s, provider_health h"
        ),
        "partial:partial:provider:provider"
    );
}

#[test]
fn explicit_refusals_fail_and_only_delivered_text_becomes_a_partial_turn() {
    for wire in WIRES {
        for with_text in [false, true] {
            let fake = FakeProvider::sequence(vec![
                Scenario::Sse(wire.stream),
                Scenario::Sse(refusal(wire, with_text)),
                Scenario::Sse(wire.stream),
            ]);
            let home = configured(&fake, wire, STANDARD);
            assert!(ask(&home, &["first"]).status.success());
            let output = ask(&home, &["reply", "refused"]);
            assert_eq!(output.status.code(), Some(1), "{}: {output:?}", wire.kind);
            assert_eq!(
                output.stdout,
                if with_text {
                    &b"cannot comply\n"[..]
                } else {
                    &[][..]
                },
                "{}",
                wire.kind
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                "ask: provider declined to answer because of content filtering\n",
                "{}",
                wire.kind
            );
            let expected = if with_text { "2:partial" } else { "1:complete" };
            assert_eq!(
                row(
                    &home,
                    "SELECT count(*) || ':' || (SELECT status FROM turns ORDER BY id DESC LIMIT 1) FROM turns"
                ),
                expected,
                "{}",
                wire.kind
            );
            assert_eq!(
                row(
                    &home,
                    "SELECT outcome || ':' || error_class || ':' || input_tokens || ':' || output_tokens FROM query_statistics ORDER BY id DESC LIMIT 1"
                ),
                if with_text {
                    "partial:provider:12:3"
                } else {
                    "failed:provider:12:3"
                },
                "{}",
                wire.kind
            );
            assert!(ask(&home, &["reply", "next"]).status.success());
            let roles: Vec<_> = conversation(wire, &fake.requests(3)[2])
                .into_iter()
                .map(|(role, _)| role)
                .collect();
            assert_eq!(
                roles,
                ["system", "user", "assistant", "user"],
                "{}",
                wire.kind
            );
        }
    }
}

#[test]
fn ordinary_empty_answers_are_complete_turns() {
    for wire in WIRES {
        let fake = FakeProvider::start(Scenario::Sse(empty_success(wire)));
        let home = configured(&fake, wire, STANDARD);
        let output = ask(&home, &["empty"]);
        assert!(output.status.success(), "{}: {output:?}", wire.kind);
        assert_eq!(output.stdout, b"\n", "{}", wire.kind);
        assert_eq!(
            row(
                &home,
                "SELECT status || ':' || length(answer) || ':' || s.outcome || ':' || s.input_tokens || ':' || s.output_tokens FROM turns, query_statistics s"
            ),
            "complete:0:complete:12:0",
            "{}",
            wire.kind
        );
    }
}

#[test]
fn gemini_prompt_feedback_without_candidates_is_a_refusal() {
    let fake = FakeProvider::start(Scenario::Sse(GEMINI_PROMPT_REFUSAL));
    let home = configured(&fake, GEMINI, STANDARD);
    let output = ask(&home, &["question"]);
    assert_failure(
        &output,
        ExpectedFailure {
            stdout: b"",
            stderr: "ask: provider declined to answer because of content filtering\n",
        },
        GEMINI,
    );
    assert_eq!(
        row(
            &home,
            "SELECT outcome || ':' || error_class || ':' || input_tokens || ':' || output_tokens FROM query_statistics"
        ),
        "failed:provider:12:0"
    );
}

#[test]
fn gemini_thought_parts_are_not_answer_text() {
    let fake = FakeProvider::start(Scenario::Sse(GEMINI_THOUGHT_AND_ANSWER));
    let home = configured(&fake, GEMINI, STANDARD);
    let output = ask(&home, &["question"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"visible answer\n");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("hidden"));
}

#[test]
fn gemini_recitation_is_an_unsuccessful_refusal() {
    let stream = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"quoted\"}],\"role\":\"model\"},\"finishReason\":\"RECITATION\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":3,\"totalTokenCount\":15}}\n\n";
    let fake = FakeProvider::start(Scenario::Sse(stream));
    let home = configured(&fake, GEMINI, STANDARD);
    let output = ask(&home, &["question"]);
    assert_failure(
        &output,
        ExpectedFailure {
            stdout: b"quoted\n",
            stderr: "ask: provider declined to answer because of content filtering\n",
        },
        GEMINI,
    );
    assert_eq!(row(&home, "SELECT status FROM turns"), "partial");
}

#[test]
fn gemini_unknown_finish_reasons_are_provider_failures() {
    let stream = "data: {\"candidates\":[{\"content\":{\"parts\":[],\"role\":\"model\"},\"finishReason\":\"FUTURE_REASON\",\"index\":0}]}\n\n";
    let fake = FakeProvider::start(Scenario::Sse(stream));
    let home = configured(&fake, GEMINI, STANDARD);
    let output = ask(&home, &["question"]);
    assert_failure(
        &output,
        ExpectedFailure {
            stdout: b"",
            stderr: "ask: provider request failed: unrecognized Gemini finish reason: FUTURE_REASON\n",
        },
        GEMINI,
    );
}

#[test]
fn gemini_error_and_protocol_finish_reasons_are_provider_failures() {
    for (reason, stream) in [
        (
            "MALFORMED_FUNCTION_CALL",
            "data: {\"candidates\":[{\"content\":{\"parts\":[],\"role\":\"model\"},\"finishReason\":\"MALFORMED_FUNCTION_CALL\",\"index\":0}]}\n\n",
        ),
        (
            "FINISH_REASON_UNSPECIFIED",
            "data: {\"candidates\":[{\"content\":{\"parts\":[],\"role\":\"model\"},\"finishReason\":\"FINISH_REASON_UNSPECIFIED\",\"index\":0}]}\n\n",
        ),
    ] {
        let fake = FakeProvider::start(Scenario::Sse(stream));
        let home = configured(&fake, GEMINI, STANDARD);
        let output = ask(&home, &["question"]);
        assert_eq!(output.status.code(), Some(1), "{reason}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(reason),
            "{reason}: {output:?}"
        );
    }
}

#[test]
fn gemini_usage_after_the_terminal_candidate_is_retained() {
    let fake = FakeProvider::start(Scenario::Sse(GEMINI_LATE_USAGE));
    let home = configured(&fake, GEMINI, STANDARD);
    let output = ask(&home, &["question"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"answer\n");
    assert_eq!(
        row(
            &home,
            "SELECT input_tokens || ':' || output_tokens FROM query_statistics"
        ),
        "12:3"
    );
}

#[test]
fn gemini_sse_handles_crlf_and_http_chunk_boundaries() {
    const PARTS: &[&str] = &[
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"ans",
        "wer\"}],\"role\":\"model\"},\"finishReason\":\"STOP\",\"index\":0}],",
        "\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":3,\"totalTokenCount\":15}}\r",
        "\n\r\n",
    ];
    let fake = FakeProvider::start(Scenario::SseChunks(PARTS));
    let home = configured(&fake, GEMINI, STANDARD);
    let output = ask(&home, &["question"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"answer\n");
}

#[test]
fn gemini_malformed_and_unterminated_records_are_provider_failures() {
    for stream in [
        "data: not-json\n\n",
        "data: {\"candidates\":[{\"finishReason\":\"STOP\"}]}\n",
    ] {
        let fake = FakeProvider::start(Scenario::Sse(stream));
        let home = configured(&fake, GEMINI, STANDARD);
        let output = ask(&home, &["question"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).starts_with("ask: provider request failed:"),
            "{output:?}"
        );
    }
}

#[test]
fn streams_without_a_terminal_record_are_partial_and_not_replayed() {
    for wire in WIRES {
        let end = if wire.kind == "anthropic" {
            wire.stream.find("event: content_block_stop").unwrap()
        } else {
            wire.stream.find("\n\n").unwrap() + 2
        };
        let fake = FakeProvider::sequence(vec![
            Scenario::Sse(&wire.stream[..end]),
            Scenario::Sse(wire.stream),
        ]);
        let home = configured(&fake, wire, STANDARD);
        let output = ask(&home, &["question"]);
        assert_eq!(
            (
                output.status.code(),
                output.stdout.is_empty(),
                String::from_utf8_lossy(&output.stderr).contains("completion marker"),
            ),
            (Some(1), false, true),
            "{}: {output:?}",
            wire.kind
        );
        assert_eq!(row(&home, "SELECT status FROM turns"), "partial");
        assert!(ask(&home, &["reply", "next"]).status.success());
        let roles: Vec<_> = conversation(wire, &fake.requests(2)[1])
            .into_iter()
            .map(|(role, _)| role)
            .collect();
        assert_eq!(roles, ["system", "user"], "{}", wire.kind);
    }
}

#[test]
fn provider_errors_never_show_the_credential() {
    for wire in WIRES {
        for (status, body) in [
            (401, ECHOED_CREDENTIAL),
            (429, "{\"error\":{\"message\":\"rate limited\"}}"),
        ] {
            let fake = FakeProvider::start(Scenario::Status(status, body));
            let home = configured(&fake, wire, STANDARD);
            let output = ask(&home, &["question"]);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                (
                    output.status.code(),
                    output.stdout.is_empty(),
                    stderr.contains(&status.to_string()),
                    stderr.contains(CREDENTIAL),
                    stderr.lines().count(),
                ),
                (Some(1), true, true, false, 1),
                "{}: {stderr}",
                wire.kind
            );
        }
    }
}

#[test]
fn json_escaped_credentials_are_redacted_from_provider_errors() {
    let credential = "fixture/key-é";
    let body = r#"{"error":{"message":"bad fixture\/key-\u00e9"}}"#;
    let fake = FakeProvider::start(Scenario::Status(401, body));
    let home = configured(&fake, COMPATIBLE, STANDARD);
    let output = command(&home, true)
        .env("LOCAL_API_KEY", credential)
        .arg("question")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("[redacted]"), "{stderr}");
    for shown in [credential, r"fixture\/key-\u00e9", "fixture/key"] {
        assert!(!stderr.contains(shown), "{stderr}");
    }
}

#[test]
fn every_kind_refuses_redirects_without_contacting_the_destination() {
    for wire in WIRES {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let port = Some(destination.local_addr().unwrap().port());
        let fake = FakeProvider::start(Scenario::Redirect { status: 307, port });
        let home = configured(&fake, wire, STANDARD);
        let output = ask(&home, &["PRIVATE_QUERY"]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let destination_error = destination.accept().unwrap_err().kind();
        assert_eq!(
            (
                output.status.code(),
                stderr.contains("307"),
                stderr.contains(CREDENTIAL),
                fake.connections(),
                destination_error,
            ),
            (Some(1), true, false, 1, std::io::ErrorKind::WouldBlock),
            "{}: {stderr}",
            wire.kind
        );
    }
}

#[test]
fn gemini_model_identifiers_stay_one_encoded_path_segment() {
    let fake = FakeProvider::start(Scenario::Sse(GEMINI_STREAM));
    let home = configured(
        &fake,
        GEMINI,
        Setup {
            model: "tuned/../m?x=1#f :é",
            profile: "",
        },
    );
    let output = ask(&home, &["question"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fake.recorded().unwrap().path,
        format!(
            "/v1beta/models/tuned%2F..%2Fm%3Fx%3D1%23f%20%3A%C3%A9:streamGenerateContent?alt=sse&key={CREDENTIAL}"
        )
    );
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("ask: tuned/../m?x=1#f :é · "));
}

#[test]
fn gemini_credentials_are_encoded_in_the_url_and_redacted_from_errors() {
    let credential = "fixture'key é/+&#%";
    let encoded = "fixture%27key%20%C3%A9%2F%2B%26%23%25";
    let fake = FakeProvider::start(Scenario::Sse(GEMINI_STREAM));
    let home = configured(&fake, GEMINI, STANDARD);
    let output = command(&home, true)
        .env("LOCAL_API_KEY", credential)
        .arg("question")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(
        fake.recorded()
            .unwrap()
            .path
            .ends_with(&format!("?alt=sse&key={encoded}"))
    );
    drop(fake);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let home = configured_url(&origin, GEMINI, STANDARD);
    let output = command(&home, true)
        .env("LOCAL_API_KEY", credential)
        .arg("question")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("key=[redacted]"), "{stderr}");
    for shown in [credential, encoded, "fixture'key", "fixture%27key"] {
        assert!(!stderr.contains(shown), "{stderr}");
    }
}

#[test]
fn replies_continue_threads_captured_before_output_limits_existed() {
    let fake = FakeProvider::start(Scenario::Sse(CHAT_STREAM));
    let home = fresh_home();
    fs::create_dir_all(home.join("data")).unwrap();
    let base_url = format!("{}/v1", origin(&fake));
    rusqlite::Connection::open(home.join("data/ask.sqlite3"))
        .unwrap()
        .execute_batch(&format!(
            "
CREATE TABLE threads (id INTEGER PRIMARY KEY, created_at_ms INTEGER NOT NULL, profile TEXT NOT NULL, provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, system_prompt TEXT NOT NULL, timeout_ms INTEGER NOT NULL CHECK (timeout_ms > 0), api_key_env TEXT NOT NULL);
CREATE TABLE turns (id INTEGER PRIMARY KEY, thread_id INTEGER NOT NULL REFERENCES threads (id) ON DELETE CASCADE, ordinal INTEGER NOT NULL CHECK (ordinal > 0), created_at_ms INTEGER NOT NULL, prompt TEXT NOT NULL, answer TEXT NOT NULL, status TEXT NOT NULL CHECK (status IN ('complete', 'partial')), reason TEXT CHECK ((status = 'complete') = (reason IS NULL)), UNIQUE (thread_id, ordinal));
CREATE TABLE current_thread (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), thread_id INTEGER NOT NULL REFERENCES threads (id) ON DELETE CASCADE);
CREATE TABLE query_statistics (id INTEGER PRIMARY KEY, started_at_ms INTEGER NOT NULL, command TEXT NOT NULL CHECK (command IN ('new', 'reply')), profile TEXT NOT NULL, provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, outcome TEXT NOT NULL CHECK (outcome IN ('complete', 'partial', 'failed')), error_class TEXT, wall_ms INTEGER NOT NULL, api_ms INTEGER NOT NULL, first_token_ms INTEGER, input_tokens INTEGER, output_tokens INTEGER);
CREATE TABLE provider_health (provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, last_success_at_ms INTEGER, last_failure_at_ms INTEGER, last_failure_class TEXT, PRIMARY KEY (provider_kind, base_url, model)) WITHOUT ROWID;
INSERT INTO threads VALUES (1, 1, 'default', 'openai-compatible', '{base_url}', 'old-model', 'Old prompt.', 30000, 'LOCAL_API_KEY');
INSERT INTO turns VALUES (1, 1, 1, 1, 'old question', 'old answer', 'complete', NULL);
INSERT INTO current_thread VALUES (1, 1);
PRAGMA user_version = 1;
"
        ))
        .unwrap();
    let output = ask(&home, &["reply", "new question"]);
    assert!(output.status.success(), "{output:?}");
    let request = fake.recorded().unwrap();
    assert_eq!(request.model, "old-model");
    assert_eq!(request.body.get("max_tokens"), None);
    assert_eq!(
        request.messages,
        [
            ("system", "Old prompt."),
            ("user", "old question"),
            ("assistant", "old answer"),
            ("user", "new question"),
        ]
        .map(|(role, text)| (role.to_string(), text.to_string()))
    );
    assert_eq!(
        row(
            &home,
            "SELECT (SELECT user_version FROM pragma_user_version) || ':' || count(*) FROM turns"
        ),
        "4:2"
    );
}

fn origin(fake: &FakeProvider) -> String {
    format!("http://{}", fake.address())
}

fn configured(fake: &FakeProvider, wire: Wire, setup: Setup) -> PathBuf {
    let base_url = format!("{}{}", origin(fake), wire.base_path);
    configured_url(&base_url, wire, setup)
}

/// A home whose default profile uses `kind` at `base_url`; `profile` holds
/// extra profile lines.
fn configured_url(base_url: &str, wire: Wire, setup: Setup) -> PathBuf {
    let home = fresh_home();
    let (kind, model, profile) = (wire.kind, setup.model, setup.profile);
    let config = format!(
        "default_profile = \"default\"\n\n[providers.p]\nkind = \"{kind}\"\nbase_url = \"{base_url}\"\napi_key_env = \"LOCAL_API_KEY\"\ntimeout_ms = 5000\n\n[profiles.default]\nprovider = \"p\"\nmodel = '{model}'\n{profile}"
    );
    fs::write(home.join("config.toml"), config).unwrap();
    home
}

fn ask(home: &Path, args: &[&str]) -> Output {
    command(home, true)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}

fn row(home: &Path, query: &str) -> String {
    rusqlite::Connection::open(home.join("data/ask.sqlite3"))
        .unwrap()
        .query_row(query, [], |row| row.get(0))
        .unwrap()
}
