//! End-to-end proof that a leading inline `<think>...</think>` block is not
//! part of the answer on any provider kind: it never reaches stdout or the
//! recorded turn and is never sent back in a reply, while every other answer
//! text is delivered byte for byte. Runs against the loopback fake with an
//! `openai-compatible` and an `anthropic` wire format.

mod support;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use rig_core::serde_json::{Value, json};
use support::{
    command,
    fake_provider::{FakeProvider, Scenario},
    fresh_home,
};

const WARNING: &str = "ask: warning: answer stopped at the provider's output-token limit; set a larger max_output_tokens in the profile\n";
const UNCLOSED: [&str; 2] = ["<think>never", " closed"];
const BLOCK_PARTS: [&str; 6] = [
    "<thi",
    "nk>secret rea",
    "soning</th",
    "ink>\n\n",
    "The ",
    "answer",
];

#[derive(Clone, Copy, Debug)]
enum Ending {
    Complete,
    OutputLimit,
    /// The stream stops with no terminal record.
    Cut,
}

#[derive(Clone, Copy, Debug)]
enum Wire {
    Compatible,
    Anthropic,
}

const WIRES: [Wire; 2] = [Wire::Compatible, Wire::Anthropic];

impl Wire {
    fn kind(self) -> &'static str {
        match self {
            Self::Compatible => "openai-compatible",
            Self::Anthropic => "anthropic",
        }
    }

    fn base_path(self) -> &'static str {
        match self {
            Self::Compatible => "/v1",
            Self::Anthropic => "",
        }
    }

    /// A stream delivering `parts` as separate text deltas.
    fn stream(self, parts: &[&str], ending: Ending) -> Scenario {
        let body = match self {
            Self::Compatible => chat_body(parts, ending),
            Self::Anthropic => anthropic_body(parts, ending),
        };
        Scenario::Sse(Box::leak(body.into_boxed_str()))
    }
}

fn chat_body(parts: &[&str], ending: Ending) -> String {
    let event = |delta: Value, finish: Value| {
        let chunk = json!({"id": "c", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}], "usage": null});
        format!("data: {chunk}\n\n")
    };
    let mut body: String = parts
        .iter()
        .map(|part| event(json!({"content": part}), Value::Null))
        .collect();
    let reason = match ending {
        Ending::Complete => "stop",
        Ending::OutputLimit => "length",
        Ending::Cut => return body,
    };
    body += &event(json!({}), json!(reason));
    body + "data: [DONE]\n\n"
}

fn anthropic_body(parts: &[&str], ending: Ending) -> String {
    let event = |name: &str, data: Value| format!("event: {name}\ndata: {data}\n\n");
    let message = json!({"id": "m", "type": "message", "role": "assistant", "model": "m", "content": [], "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 12, "output_tokens": 1}});
    let mut body = event(
        "message_start",
        json!({"type": "message_start", "message": message}),
    );
    body += &event(
        "content_block_start",
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
    );
    for part in parts {
        body += &event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": part}}),
        );
    }
    let reason = match ending {
        Ending::Complete => "end_turn",
        Ending::OutputLimit => "max_tokens",
        Ending::Cut => return body,
    };
    body += &event(
        "content_block_stop",
        json!({"type": "content_block_stop", "index": 0}),
    );
    body += &event(
        "message_delta",
        json!({"type": "message_delta", "delta": {"stop_reason": reason, "stop_sequence": null}, "usage": {"output_tokens": 3}}),
    );
    body + &event("message_stop", json!({"type": "message_stop"}))
}

fn configured(fake: &FakeProvider, wire: Wire) -> PathBuf {
    let home = fresh_home();
    let (kind, path) = (wire.kind(), wire.base_path());
    let url = format!("http://{}{path}", fake.address());
    let config = format!(
        "default_profile = \"default\"\n\n[providers.p]\nkind = \"{kind}\"\nbase_url = \"{url}\"\napi_key_env = \"LOCAL_API_KEY\"\ntimeout_ms = 5000\n\n[profiles.default]\nprovider = \"p\"\nmodel = \"model\"\n"
    );
    fs::write(home.join("config.toml"), config).unwrap();
    home
}

fn ask(home: &Path, args: &[&str]) -> Output {
    command(home, true)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// Runs `ask` with stdout already closed, optionally feeding stdin.
fn ask_with_closed_stdout(home: &Path, payload: &[u8]) -> Output {
    let mut child = command(home, true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child.stdin.take().unwrap().write_all(payload).unwrap();
    child.wait_with_output().unwrap()
}

fn turns(home: &Path) -> String {
    let connection = rusqlite::Connection::open(home.join("data/ask.sqlite3")).unwrap();
    connection
        .query_row(
            "SELECT coalesce(group_concat(status || ':' || answer, '|'), 'none') FROM turns",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// What a run showed the user: exit code, stdout, and whether stderr held any
/// reasoning text.
fn shown(output: &Output) -> (Option<i32>, Vec<u8>, bool) {
    let leaked = stderr(output).contains("reasoning") || stderr(output).contains("never");
    (output.status.code(), output.stdout.clone(), leaked)
}

fn assert_no_reasoning(wire: Wire, text: &str) {
    for marker in ["reasoning", "think>", "never"] {
        assert!(!text.contains(marker), "{wire:?}: {text}");
    }
}

#[test]
fn a_leading_block_is_neither_printed_recorded_nor_replayed() {
    for wire in WIRES {
        let fake = FakeProvider::sequence(vec![
            wire.stream(&BLOCK_PARTS, Ending::Complete),
            wire.stream(&["fine"], Ending::Complete),
        ]);
        let home = configured(&fake, wire);
        let output = ask(&home, &["question"]);
        assert_eq!(shown(&output), (Some(0), b"The answer\n".to_vec(), false));
        assert_eq!(turns(&home), "complete:The answer", "{wire:?}");
        assert!(ask(&home, &["reply", "again"]).status.success());
        let replay = fake.requests(2)[1].body.to_string();
        assert!(replay.contains("The answer"), "{wire:?}: {replay}");
        assert_no_reasoning(wire, &replay);
    }
}

#[test]
fn answers_without_a_leading_tag_are_unchanged() {
    let cases: [&[&str]; 4] = [
        &["Use <think> tags", " like this\n\nok"],
        &["  <thin", "king> not a tag"],
        &["Hello ", "<think>x</think> world"],
        &["<th", "ink"],
    ];
    for wire in WIRES {
        for parts in cases {
            let fake = FakeProvider::start(wire.stream(parts, Ending::Complete));
            let home = configured(&fake, wire);
            let output = ask(&home, &["question"]);
            let answer = parts.concat();
            assert!(output.status.success(), "{wire:?}: {}", stderr(&output));
            let expected = format!("{}\n", answer.trim_end_matches('\n'));
            assert_eq!(output.stdout, expected.as_bytes(), "{wire:?} {parts:?}");
            assert_eq!(turns(&home), format!("complete:{answer}"), "{wire:?}");
        }
    }
}

/// Runs a query whose answer is an unclosed block ended as `ending` says.
fn unclosed(wire: Wire, ending: Ending) -> (Output, PathBuf) {
    let fake = FakeProvider::start(wire.stream(&UNCLOSED, ending));
    let home = configured(&fake, wire);
    (ask(&home, &["question"]), home)
}

#[test]
fn an_unclosed_block_with_a_normal_ending_is_a_complete_empty_answer() {
    for wire in WIRES {
        let (output, home) = unclosed(wire, Ending::Complete);
        assert_eq!(shown(&output), (Some(0), b"\n".to_vec(), false), "{wire:?}");
        assert_eq!(turns(&home), "complete:", "{wire:?}");
    }
}

#[test]
fn an_unclosed_block_in_a_failed_stream_prints_and_records_nothing() {
    for wire in WIRES {
        let (output, home) = unclosed(wire, Ending::Cut);
        assert_eq!(shown(&output), (Some(1), Vec::new(), false), "{wire:?}");
        assert!(stderr(&output).contains("completion marker"), "{wire:?}");
        assert_eq!(turns(&home), "none", "{wire:?}");
    }
}

#[test]
fn an_unclosed_block_at_the_output_limit_is_a_partial_empty_answer() {
    for wire in WIRES {
        let (output, home) = unclosed(wire, Ending::OutputLimit);
        assert_eq!(shown(&output), (Some(1), Vec::new(), false), "{wire:?}");
        assert_eq!(stderr(&output), WARNING, "{wire:?}");
        assert_eq!(turns(&home), "partial:", "{wire:?}");
    }
}

#[test]
fn a_closed_stdout_is_quiet_inside_and_after_a_block() {
    let inside: &[&str] = &UNCLOSED;
    for wire in WIRES {
        for (parts, answer) in [(inside, ""), (&BLOCK_PARTS[..], "The answer")] {
            let fake = FakeProvider::start(wire.stream(parts, Ending::Complete));
            let home = configured(&fake, wire);
            let output = ask_with_closed_stdout(&home, b"question");
            assert!(output.status.success(), "{wire:?}: {}", stderr(&output));
            assert!(output.stderr.is_empty(), "{wire:?}: {}", stderr(&output));
            let expected = format!("partial:{answer}");
            assert_eq!(turns(&home), expected, "{wire:?}");
        }
    }
}

#[test]
fn piped_input_composed_with_arguments_gets_the_same_treatment() {
    for wire in WIRES {
        let fake = FakeProvider::start(wire.stream(&BLOCK_PARTS, Ending::Complete));
        let home = configured(&fake, wire);
        let mut child = command(&home, true)
            .args(["summarize", "this"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"piped text")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{wire:?}: {}", stderr(&output));
        assert_eq!(output.stdout, b"The answer\n", "{wire:?}");
        let request = fake.recorded().unwrap().body.to_string();
        assert!(request.contains("piped text"), "{wire:?}: {request}");
        assert_eq!(turns(&home), "complete:The answer", "{wire:?}");
    }
}
