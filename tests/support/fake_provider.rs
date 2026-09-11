use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rig_core::serde_json::{Value, json};

static FAKE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy)]
pub enum Scenario {
    Stream,
    StreamWithoutUsage,
    Answer(&'static str),
    Empty,
    Stall,
    Malformed,
    Unauthorized,
    RateLimited,
    PartialFailure,
}

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub path: String,
    pub authorization_present: bool,
    pub model: String,
    pub messages: Vec<(String, String)>,
}

/// Holds each response after its request is recorded until the gate opens.
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn new(open: bool) -> Self {
        Self {
            open: Mutex::new(open),
            changed: Condvar::new(),
        }
    }

    fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }

    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

struct Shared {
    recorded: Mutex<Vec<RecordedRequest>>,
    gate: Gate,
    /// The one request whose response waits for the gate, or `None` when
    /// every response waits for it.
    held: Option<usize>,
    stop: AtomicBool,
}

/// A loopback provider that answers successive connections with successive
/// scenarios, repeating the last one, and records every request.
pub struct FakeProvider {
    _serial: MutexGuard<'static, ()>,
    address: SocketAddr,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl FakeProvider {
    pub fn start(scenario: Scenario) -> Self {
        Self::launch(vec![scenario], true, None)
    }

    pub fn sequence(scenarios: Vec<Scenario>) -> Self {
        Self::launch(scenarios, true, None)
    }

    /// Records requests but withholds every response until [`Self::release`].
    pub fn gated(scenario: Scenario) -> Self {
        Self::launch(vec![scenario], false, None)
    }

    /// Like [`Self::sequence`], but withholds only the response to the
    /// zero-based request `held` until [`Self::release`], answering later
    /// requests meanwhile.
    pub fn holding(scenarios: Vec<Scenario>, held: usize) -> Self {
        Self::launch(scenarios, false, Some(held))
    }

    fn launch(scenarios: Vec<Scenario>, open: bool, held: Option<usize>) -> Self {
        let serial = FAKE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let shared = Arc::new(Shared {
            recorded: Mutex::new(Vec::new()),
            gate: Gate::new(open),
            held,
            stop: AtomicBool::new(false),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = thread::spawn(move || serve(&listener, &scenarios, &thread_shared));
        Self {
            _serial: serial,
            address,
            shared,
            thread: Some(thread),
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.address)
    }

    pub fn release(&self) {
        self.shared.gate.release();
    }

    /// The first request, waiting briefly for it to arrive.
    pub fn recorded(&self) -> Option<RecordedRequest> {
        self.wait_for(1, Duration::from_secs(1)).into_iter().next()
    }

    /// Every request so far, waiting up to five seconds for `count` of them.
    pub fn requests(&self, count: usize) -> Vec<RecordedRequest> {
        self.wait_for(count, Duration::from_secs(5))
    }

    fn wait_for(&self, count: usize, limit: Duration) -> Vec<RecordedRequest> {
        let deadline = Instant::now() + limit;
        loop {
            let recorded = self.shared.recorded.lock().unwrap().clone();
            if recorded.len() >= count || Instant::now() >= deadline {
                return recorded;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.gate.release();
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(listener: &TcpListener, scenarios: &[Scenario], shared: &Arc<Shared>) {
    for connection in listener.incoming() {
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(mut stream) = connection else { continue };
        let Some(request) = read_request(&mut stream) else {
            continue;
        };
        let index = {
            let mut recorded = shared.recorded.lock().unwrap();
            recorded.push(request);
            recorded.len() - 1
        };
        answer(
            stream,
            scenarios[index.min(scenarios.len() - 1)],
            index,
            shared,
        );
    }
}

/// Answers a singly held request on its own thread so later requests are
/// served while it waits.
fn answer(mut stream: TcpStream, scenario: Scenario, index: usize, shared: &Arc<Shared>) {
    match shared.held {
        Some(held) if held == index => {
            let shared = Arc::clone(shared);
            thread::spawn(move || {
                shared.gate.wait();
                respond(&mut stream, scenario);
            });
        }
        Some(_) => respond(&mut stream, scenario),
        None => {
            shared.gate.wait();
            respond(&mut stream, scenario);
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let bytes = read_http_message(stream)?;
    let split = find(&bytes, b"\r\n\r\n")?;
    let headers = String::from_utf8_lossy(&bytes[..split]);
    let body = &bytes[split + 4..];
    let path = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))?
        .to_string();
    let authorization_present = headers
        .lines()
        .any(|line| line.to_ascii_lowercase().starts_with("authorization:"));
    let value: Value = rig_core::serde_json::from_slice(body).ok()?;
    Some(RecordedRequest {
        path,
        authorization_present,
        model: value["model"].as_str()?.to_string(),
        messages: messages(&value),
    })
}

/// Reads one HTTP request. Returns `None` when the client closes or stalls
/// before the request is complete, so the server thread always terminates.
fn read_http_message(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = stream.read(&mut buffer).ok()?;
        if count == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..count]);
        let Some(split) = find(&bytes, b"\r\n\r\n") else {
            continue;
        };
        let content_length = content_length(&bytes[..split]);
        if bytes.len() >= split + 4 + content_length {
            return Some(bytes);
        }
    }
}

fn content_length(headers: &[u8]) -> usize {
    String::from_utf8_lossy(headers)
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(str::trim)
                .and_then(|value| value.parse().ok())
        })
        .unwrap()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|part| part == needle)
}

fn messages(value: &Value) -> Vec<(String, String)> {
    value["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|message| {
            let role = message["role"].as_str().unwrap().to_string();
            (role, content(&message["content"]))
        })
        .collect()
}

fn content(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    value
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect()
}

fn respond(stream: &mut TcpStream, scenario: Scenario) {
    match scenario {
        Scenario::Stream => stream_answer(stream, &["**", "4**\n\n"], true),
        Scenario::StreamWithoutUsage => stream_answer(stream, &["**", "4**\n\n"], false),
        Scenario::Answer(text) => stream_answer(stream, &[text], true),
        Scenario::Empty => stream_answer(stream, &[], true),
        Scenario::Stall => thread::sleep(Duration::from_millis(2_000)),
        Scenario::Malformed => fixed(stream, 200, "text/event-stream", "data: not-json\n\n"),
        Scenario::Unauthorized => error(stream, 401, "unauthorized"),
        Scenario::RateLimited => error(stream, 429, "rate limited"),
        Scenario::PartialFailure => partial_failure(stream),
    }
}

fn stream_answer(stream: &mut TcpStream, parts: &[&str], include_usage: bool) {
    chunked_headers(stream);
    for part in parts {
        chunk(stream, &content_event(part));
    }
    let mut last = json!({
        "id": "chatcmpl-1",
        "object": "chat.completion.chunk",
        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
    });
    if include_usage {
        last["usage"] = json!({"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15});
    }
    chunk(stream, &format!("data: {last}\n\n"));
    chunk(stream, "data: [DONE]\n\n");
    let _ = stream.write_all(b"0\r\n\r\n");
}

fn content_event(text: &str) -> String {
    let event = json!({
        "id": "chatcmpl-1",
        "object": "chat.completion.chunk",
        "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": null}],
    });
    format!("data: {event}\n\n")
}

fn partial_failure(stream: &mut TcpStream) {
    chunked_headers(stream);
    chunk(stream, &content_event("partial"));
    let _ = stream.write_all(b"not-a-size\r\n");
}

fn chunked_headers(stream: &mut TcpStream) {
    let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
    );
}

fn chunk(stream: &mut TcpStream, body: &str) {
    let header = format!("{:x}\r\n", body.len());
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.write_all(b"\r\n");
    let _ = stream.flush();
}

fn error(stream: &mut TcpStream, status: u16, message: &str) {
    let body = format!("{{\"error\":{{\"message\":\"{message}\"}}}}");
    fixed(stream, status, "application/json", &body);
}

fn fixed(stream: &mut TcpStream, status: u16, content_type: &str, body: &str) {
    let reason = if status == 200 { "OK" } else { "Error" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}
