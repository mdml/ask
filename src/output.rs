use std::io::{self, Write};

pub struct AnswerWriter<'a, W> {
    output: &'a mut W,
    trailing_line_endings: String,
    received_text: bool,
}

impl<'a, W: Write> AnswerWriter<'a, W> {
    pub fn new(output: &'a mut W) -> Self {
        Self {
            output,
            trailing_line_endings: String::new(),
            received_text: false,
        }
    }

    pub fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
        self.received_text |= !chunk.is_empty();
        let body_len = chunk.trim_end_matches(['\r', '\n']).len();
        let (body, endings) = chunk.split_at(body_len);
        if !body.is_empty() {
            self.flush_pending()?;
            self.output.write_all(body.as_bytes())?;
        }
        self.trailing_line_endings.push_str(endings);
        Ok(())
    }

    pub fn finish(&mut self, successful: bool) -> io::Result<()> {
        self.trailing_line_endings.clear();
        if successful || self.received_text {
            self.output.write_all(b"\n")?;
        }
        self.output.flush()
    }

    fn flush_pending(&mut self) -> io::Result<()> {
        self.output
            .write_all(self.trailing_line_endings.as_bytes())?;
        self.trailing_line_endings.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_one_final_newline() {
        let mut output = Vec::new();
        let mut writer = AnswerWriter::new(&mut output);
        writer.write_chunk("answer").unwrap();
        writer.finish(true).unwrap();
        assert_eq!(output, b"answer\n");
    }

    #[test]
    fn collapses_trailing_line_endings_only() {
        let mut output = Vec::new();
        let mut writer = AnswerWriter::new(&mut output);
        writer.write_chunk("one\n\n").unwrap();
        writer.write_chunk("two\r\n\n").unwrap();
        writer.finish(true).unwrap();
        assert_eq!(output, b"one\n\ntwo\n");
    }

    #[test]
    fn failed_empty_answer_stays_empty() {
        let mut output = Vec::new();
        AnswerWriter::new(&mut output).finish(false).unwrap();
        assert!(output.is_empty());
    }
}
