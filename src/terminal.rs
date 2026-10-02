//! Small terminal presentation primitives backed by `console`.

use std::io;

use console::{Key, Term};

use crate::menu_filter::{Edit, Filter};

/// Selects one safe, single-line label with arrow keys. Escape cancels.
///
/// `labels` must be nonempty.
pub fn select(instruction: &str, labels: &[String]) -> io::Result<Option<usize>> {
    let term = Term::stderr();
    let rows: Vec<usize> = (0..labels.len()).collect();
    let mut selected = 0;
    loop {
        let drawn = draw(
            &term,
            &[instruction],
            &Rows {
                labels,
                rows: &rows,
            },
            selected,
        )?;
        match term.read_key() {
            Ok(Key::ArrowUp) => selected = selected.checked_sub(1).unwrap_or(labels.len() - 1),
            Ok(Key::ArrowDown) => selected = (selected + 1) % labels.len(),
            Ok(Key::Enter) => {
                term.clear_last_lines(drawn)?;
                return Ok(Some(selected));
            }
            Ok(Key::Escape) => {
                term.clear_last_lines(drawn)?;
                return Ok(None);
            }
            Ok(_) => {}
            Err(error) => return Err(error),
        }
        term.clear_last_lines(drawn)?;
    }
}

/// Like [`select`], but typing narrows the labels by case-insensitive
/// substring and Backspace edits the filter. The last `pinned` labels stay
/// visible whatever is typed.
pub fn select_filtered(
    instruction: &str,
    labels: &[String],
    pinned: usize,
) -> io::Result<Option<usize>> {
    let term = Term::stderr();
    let mut filter = Filter::new(labels, pinned);
    loop {
        let header = format!("Filter: {}", filter.query());
        let rows = filter.visible();
        let drawn = draw(
            &term,
            &[instruction, &header],
            &Rows {
                labels,
                rows: &rows,
            },
            filter.selected(),
        )?;
        let key = term.read_key()?;
        term.clear_last_lines(drawn)?;
        match (key, filter.choice()) {
            (Key::Enter, Some(choice)) => return Ok(Some(choice)),
            (Key::Escape, _) => return Ok(None),
            (key, _) => edit(key).into_iter().for_each(|edit| filter.apply(edit)),
        }
    }
}

fn edit(key: Key) -> Option<Edit> {
    match key {
        Key::ArrowUp => Some(Edit::Up),
        Key::ArrowDown => Some(Edit::Down),
        Key::Backspace => Some(Edit::Erase),
        Key::Char(character) if !character.is_control() => Some(Edit::Type(character)),
        _ => None,
    }
}

/// Reads one line without echoing it. Enter submits; Escape or Ctrl-D
/// returns `None`. Ctrl-C restores the terminal and raises `SIGINT`.
pub fn read_hidden(prompt: &str) -> io::Result<Option<String>> {
    let term = Term::stderr();
    term.write_str(prompt)?;
    let mut line = String::new();
    loop {
        match term.read_key()? {
            Key::Enter => break,
            Key::Escape | Key::Char('\u{4}') => {
                term.write_line("")?;
                return Ok(None);
            }
            Key::Backspace => {
                line.pop();
            }
            Key::Char(character) if !character.is_control() => line.push(character),
            _ => {}
        }
    }
    term.write_line("")?;
    Ok(Some(line))
}

/// The labels to draw, by position in `labels`.
struct Rows<'a> {
    labels: &'a [String],
    rows: &'a [usize],
}

fn draw(term: &Term, header: &[&str], rows: &Rows<'_>, selected: usize) -> io::Result<usize> {
    let (height, width) = term.size();
    let minimum = header.len() + 2;
    if usize::from(height) < minimum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("interactive selection requires terminal height of at least {minimum} rows"),
        ));
    }
    let width = usize::from(width.max(1)).saturating_sub(1);
    let visible_labels = (usize::from(height) - minimum + 1).min(rows.rows.len());
    let first = selected.saturating_add(1).saturating_sub(visible_labels);
    let last = (first + visible_labels).min(rows.rows.len());
    let mut drawn = 0;
    for line in header {
        drawn += write_line(term, line, width)?;
    }
    for (position, &index) in rows.rows.iter().enumerate().take(last).skip(first) {
        let marker = if position == selected { '>' } else { ' ' };
        let line = format!("{marker} {}. {}", position + 1, rows.labels[index]);
        drawn += write_line(term, &line, width)?;
    }
    Ok(drawn)
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
