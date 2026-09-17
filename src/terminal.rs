//! Small terminal presentation primitives backed by `console`.

use std::io;

use console::{Key, Term};

/// Selects one safe, single-line label with arrow keys. Escape cancels.
///
/// `labels` must be nonempty.
pub fn select(instruction: &str, labels: &[String]) -> io::Result<Option<usize>> {
    let term = Term::stderr();
    let mut selected = 0;
    loop {
        let rows = draw(&term, instruction, labels, selected)?;
        match term.read_key() {
            Ok(Key::ArrowUp) => selected = selected.checked_sub(1).unwrap_or(labels.len() - 1),
            Ok(Key::ArrowDown) => selected = (selected + 1) % labels.len(),
            Ok(Key::Enter) => {
                term.clear_last_lines(rows)?;
                return Ok(Some(selected));
            }
            Ok(Key::Escape) => {
                term.clear_last_lines(rows)?;
                return Ok(None);
            }
            Ok(_) => {}
            Err(error) => return Err(error),
        }
        term.clear_last_lines(rows)?;
    }
}

fn draw(term: &Term, instruction: &str, labels: &[String], selected: usize) -> io::Result<usize> {
    let (height, width) = term.size();
    if height < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "interactive selection requires terminal height of at least 3 rows",
        ));
    }
    let width = usize::from(width.max(1)).saturating_sub(1);
    let visible_labels = usize::from(height - 2).min(labels.len());
    let first = selected.saturating_add(1).saturating_sub(visible_labels);
    let last = (first + visible_labels).min(labels.len());
    let mut rows = write_line(term, instruction, width)?;
    for (index, label) in labels.iter().enumerate().take(last).skip(first) {
        let marker = if index == selected { '>' } else { ' ' };
        let line = format!("{marker} {}. {label}", index + 1);
        rows += write_line(term, &line, width)?;
    }
    Ok(rows)
}

fn write_line(term: &Term, text: &str, width: usize) -> io::Result<usize> {
    let mut used = 0;
    let text: String = text
        .chars()
        .take_while(|character| {
            // `console` counts characters when its optional unicode-width feature is disabled.
            // Two columns is a conservative bound for printable non-ASCII terminal characters.
            let character_width = if character.is_ascii() { 1 } else { 2 };
            used += character_width;
            used <= width
        })
        .collect();
    term.write_line(&text)?;
    Ok(1)
}
