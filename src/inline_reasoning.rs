//! Removes a leading inline reasoning block, `<think>...</think>`, from
//! streamed answer text so it is never printed, recorded, or replayed.

const OPEN: &[u8] = b"<think>";
const CLOSE: &[u8] = b"</think>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Only whitespace and a possible opening tag have been seen.
    Leading,
    /// Inside the block, looking for the closing tag.
    Inside,
    /// After the closing tag, dropping the whitespace that follows it.
    Trailing,
    /// The answer proper: everything passes through unchanged.
    Answer,
}

/// A state machine over answer text chunks. `push` returns the text that is
/// part of the answer so far; `finish` returns any text still withheld.
#[derive(Debug)]
pub struct InlineReasoning {
    state: State,
    /// Bytes that could still belong to the whitespace or tag being matched.
    held: String,
}

impl InlineReasoning {
    pub const fn new() -> Self {
        Self {
            state: State::Leading,
            held: String::new(),
        }
    }

    pub fn push(&mut self, chunk: &str) -> String {
        self.held.push_str(chunk);
        self.advance()
    }

    /// Ends the stream. Withheld tag-like text is ordinary answer text; an
    /// unclosed block is dropped.
    pub fn finish(&mut self) -> String {
        let held = std::mem::take(&mut self.held);
        match self.state {
            State::Leading => held,
            _ => String::new(),
        }
    }

    /// Consumes the held text in the current state, moving on as far as it
    /// allows, and returns what is now known to be answer text.
    fn advance(&mut self) -> String {
        match self.state {
            State::Leading => self.leading(),
            State::Inside => self.inside(),
            State::Trailing => self.trailing(),
            State::Answer => std::mem::take(&mut self.held),
        }
    }

    fn leading(&mut self) -> String {
        let rest = trim_whitespace(&self.held);
        let opens = rest
            .as_bytes()
            .get(..OPEN.len())
            .is_some_and(|probe| probe.eq_ignore_ascii_case(OPEN));
        if opens {
            self.held = rest[OPEN.len()..].to_string();
            self.state = State::Inside;
            return self.advance();
        }
        // Withhold while the non-whitespace part is still a proper prefix of
        // the opening tag; otherwise release everything.
        let rest = rest.as_bytes();
        if rest.len() < OPEN.len() && OPEN[..rest.len()].eq_ignore_ascii_case(rest) {
            return String::new();
        }
        self.state = State::Answer;
        std::mem::take(&mut self.held)
    }

    fn inside(&mut self) -> String {
        let bytes = self.held.as_bytes();
        let closing = bytes
            .windows(CLOSE.len())
            .position(|window| window.eq_ignore_ascii_case(CLOSE));
        if let Some(index) = closing {
            self.held = self.held[index + CLOSE.len()..].to_string();
            self.state = State::Trailing;
            return self.advance();
        }
        let keep = partial_close(bytes);
        self.held = self.held[self.held.len() - keep..].to_string();
        String::new()
    }

    fn trailing(&mut self) -> String {
        let rest = trim_whitespace(&self.held).to_string();
        self.held.clear();
        if rest.is_empty() {
            return rest;
        }
        self.state = State::Answer;
        rest
    }
}

fn trim_whitespace(text: &str) -> &str {
    text.trim_start_matches(|c: char| c.is_ascii_whitespace())
}

/// The length of the longest suffix of `bytes` that is a proper prefix of the
/// closing tag. Such a suffix is ASCII, so it starts on a character boundary.
fn partial_close(bytes: &[u8]) -> usize {
    (1..CLOSE.len())
        .rev()
        .find(|&len| {
            bytes
                .len()
                .checked_sub(len)
                .is_some_and(|start| CLOSE[..len].eq_ignore_ascii_case(&bytes[start..]))
        })
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "inline_reasoning_tests.rs"]
mod tests;
