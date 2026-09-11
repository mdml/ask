use super::*;
use std::io::Cursor;

#[test]
fn redirected_payload_is_preserved_with_and_without_instruction() {
    let payload = " \tcafé 日本語 🦀\r\n\n";
    for (instruction, expected) in [
        (None, payload.to_string()),
        (
            Some("  describe this \n"),
            format!("describe this\n\n{payload}"),
        ),
    ] {
        let mut stderr = Vec::new();
        let result = resolve(instruction, false, &mut Cursor::new(payload), &mut stderr).unwrap();
        assert_eq!(result.as_bytes(), expected.as_bytes());
        assert!(stderr.is_empty());
    }
}

#[test]
fn terminal_instruction_does_not_read_or_prompt() {
    let mut reader = Cursor::new(b"unread");
    let mut stderr = Vec::new();
    assert_eq!(
        resolve(Some(" question "), true, &mut reader, &mut stderr).unwrap(),
        "question"
    );
    assert_eq!(reader.position(), 0);
    assert!(stderr.is_empty());
}

#[test]
fn blank_instruction_is_rejected_without_reading() {
    for terminal in [true, false] {
        for instruction in ["", " \t"] {
            let mut reader = Cursor::new(b"unread");
            let error =
                resolve(Some(instruction), terminal, &mut reader, &mut Vec::new()).unwrap_err();
            assert_eq!(error.status(), ExitCode::from(2));
            assert_eq!(reader.position(), 0);
        }
    }
}

#[test]
fn multiline_prompt_preserves_lines_and_rejects_empty_submissions() {
    let mut stderr = Vec::new();
    let text = " first\nsecond\n";
    assert_eq!(
        resolve(None, true, &mut Cursor::new(text), &mut stderr).unwrap(),
        text
    );
    assert_eq!(stderr, b"ask> ");
    for terminal in [true, false] {
        for text in ["", " \t\r\n"] {
            let error =
                resolve(None, terminal, &mut Cursor::new(text), &mut Vec::new()).unwrap_err();
            assert_eq!(error.status(), ExitCode::from(2));
            assert_eq!(error.to_string(), "query must contain non-whitespace text");
        }
    }
}

#[test]
fn blank_redirected_payload_leaves_the_instruction_alone() {
    for payload in ["", " \r\n\t"] {
        assert_eq!(
            resolve(
                Some(" ask "),
                false,
                &mut Cursor::new(payload),
                &mut Vec::new()
            )
            .unwrap(),
            "ask"
        );
    }
}

#[test]
fn invalid_utf8_is_an_input_failure() {
    let error = resolve(None, false, &mut Cursor::new(b"\xff"), &mut Vec::new()).unwrap_err();
    assert_eq!(error.status(), ExitCode::FAILURE);
    assert!(error.to_string().starts_with("cannot read standard input:"));
}

struct BrokenPrompt {
    fail_flush: bool,
}

impl io::Write for BrokenPrompt {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.fail_flush {
            Ok(bytes.len())
        } else {
            Err(io::Error::other("write failed"))
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}

#[test]
fn prompt_write_and_flush_failures_do_not_read_input() {
    for fail_flush in [false, true] {
        let mut reader = Cursor::new("unread");
        let error = resolve(None, true, &mut reader, &mut BrokenPrompt { fail_flush }).unwrap_err();
        assert_eq!(error.status(), ExitCode::FAILURE);
        assert!(error.to_string().starts_with("cannot write query prompt:"));
        assert_eq!(reader.position(), 0);
    }
}
