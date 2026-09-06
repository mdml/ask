use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rig_core::serde_json::Value;

static FAKE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy)]
pub enum Scenario {
    Stream,
    StreamWithoutUsage,
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

pub struct FakeProvider {
    _serial: MutexGuard<'static, ()>,
    address: SocketAddr,
    recorded: Arc<Mutex<Option<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeProvider {
    pub fn start(scenario: Scenario) -> Self {
        let serial = FAKE_LOCK.lock().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let recorded = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_recorded = Arc::clone(&recorded);
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || serve(listener, scenario, thread_recorded, thread_stop));
        Self {
            _serial: serial,
            address,
            recorded,
            stop,
            thread: Some(thread),
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.address)
    }

    pub fn recorded(&self) -> Option<RecordedRequest> {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if let Some(request) = self.recorded.lock().unwrap().clone() {
                return Some(request);
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.recorded.lock().unwrap().clone()
    }
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    listener: TcpListener,
    scenario: Scenario,
    recorded: Arc<Mutex<Option<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
) {
    let (mut stream, _) = listener.accept().unwrap();
    if stop.load(Ordering::SeqCst) {
        return;
    }
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    *recorded.lock().unwrap() = Some(request);
    respond(&mut stream, scenario);
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
        Scenario::Stream => stream_answer(stream, true),
        Scenario::StreamWithoutUsage => stream_answer(stream, false),
        Scenario::Stall => thread::sleep(Duration::from_millis(2_000)),
        Scenario::Malformed => fixed(stream, 200, "text/event-stream", "data: not-json\n\n"),
        Scenario::Unauthorized => error(stream, 401, "unauthorized"),
        Scenario::RateLimited => error(stream, 429, "rate limited"),
        Scenario::PartialFailure => partial_failure(stream),
    }
}

fn stream_answer(stream: &mut TcpStream, include_usage: bool) {
    chunked_headers(stream);
    chunk(
        stream,
        concat!(
            "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"content\":\"**\"},\"finish_reason\":null}]}\n\n"
        ),
    );
    chunk(
        stream,
        concat!(
            "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"content\":\"4**\\n\\n\"},\"finish_reason\":null}]}\n\n"
        ),
    );
    let final_event = if include_usage {
        "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"total_tokens\":15}}\n\n"
    } else {
        "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n"
    };
    chunk(stream, final_event);
    chunk(stream, "data: [DONE]\n\n");
    let _ = stream.write_all(b"0\r\n\r\n");
}

fn partial_failure(stream: &mut TcpStream) {
    chunked_headers(stream);
    chunk(
        stream,
        "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n",
    );
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
