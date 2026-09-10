use std::{fmt, io, process::ExitCode};

#[derive(Debug)]
pub enum Error {
    Empty,
    Read(io::Error),
    Prompt(io::Error),
}

impl Error {
    pub fn status(&self) -> ExitCode {
        match self {
            Self::Empty => ExitCode::from(2),
            _ => ExitCode::FAILURE,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("query must contain non-whitespace text"),
            Self::Read(error) => write!(formatter, "cannot read standard input: {error}"),
            Self::Prompt(error) => write!(formatter, "cannot write query prompt: {error}"),
        }
    }
}

/// Resolves query input before configuration or provider execution. `None`
/// means no prompt words; an explicitly empty argument remains `Some("")`.
pub fn resolve(
    instruction: Option<&str>,
    stdin_is_terminal: bool,
    reader: &mut impl io::Read,
    stderr: &mut impl io::Write,
) -> Result<String, Error> {
    if instruction.is_some_and(|words| words.trim().is_empty()) {
        return Err(Error::Empty);
    }
    if stdin_is_terminal {
        if let Some(instruction) = instruction {
            return Ok(instruction.trim().to_string());
        }
        return multiline(reader, stderr);
    }
    let payload = read(reader)?;
    match instruction {
        Some(instruction) => Ok(compose(instruction, &payload)),
        None => submission(payload),
    }
}

// Provisional composition: a convenience, not a security boundary. A blank
// payload (for example stdin redirected from /dev/null) leaves the instruction alone.
fn compose(instruction: &str, payload: &str) -> String {
    if payload.trim().is_empty() {
        return instruction.trim().to_string();
    }
    format!("{}\n\n{payload}", instruction.trim())
}

// Provisional terminal submission: EOF submits; SIGINT keeps its default behavior.
fn multiline(reader: &mut impl io::Read, stderr: &mut impl io::Write) -> Result<String, Error> {
    stderr.write_all(b"ask> ").map_err(Error::Prompt)?;
    stderr.flush().map_err(Error::Prompt)?;
    submission(read(reader)?)
}

// Provisional empty-input policy for stdin-only and terminal submissions.
fn submission(text: String) -> Result<String, Error> {
    if text.trim().is_empty() {
        return Err(Error::Empty);
    }
    Ok(text)
}

// Read to EOF without a size cap; invalid UTF-8 is an error, never lossy text.
fn read(reader: &mut impl io::Read) -> Result<String, Error> {
    let mut text = String::new();
    reader.read_to_string(&mut text).map_err(Error::Read)?;
    Ok(text)
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
