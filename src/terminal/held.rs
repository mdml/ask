//! The prompt terminal, held in raw mode while a menu or the hidden prompt
//! reads keys.

use std::io;

use console::{Key, Term};

use super::attributes::Raw;

/// The prompt terminal, held in raw mode until dropped.
pub(super) struct Held {
    pub(super) term: Term,
    raw: Raw,
}

impl Held {
    pub(super) fn enter() -> io::Result<Self> {
        let raw = Raw::enter()?;
        Ok(Self {
            term: Term::stderr(),
            raw,
        })
    }

    /// Reads one key. Ctrl-C restores the terminal and raises `SIGINT`.
    pub(super) fn read_key(&self) -> io::Result<Key> {
        self.raw.hold()?;
        match self.term.read_key_raw()? {
            Key::CtrlC => Err(self.raw.interrupt()),
            key => Ok(key),
        }
    }
}
