//! Holds the input terminal in raw mode for the whole of one interactive
//! read. This is the crate's only `unsafe` code: it saves, changes, and
//! restores terminal attributes and raises `SIGINT` after Ctrl-C.

use std::{
    io,
    mem::MaybeUninit,
    os::fd::{AsRawFd, RawFd},
};

/// Raw mode on standard input, the terminal `console` reads keys from.
/// Dropping it restores the attributes saved when it was entered.
pub(super) struct Raw {
    fd: RawFd,
    saved: libc::termios,
    raw: libc::termios,
}

impl Raw {
    /// Saves the attributes and switches to the raw mode `console` uses for
    /// one key: no echo, no line buffering, no signal keys, output unchanged.
    pub(super) fn enter() -> io::Result<Self> {
        let fd = io::stdin().as_raw_fd();
        let saved = get(fd)?;
        let mut raw = saved;
        // SAFETY: `raw` is an initialized `termios` that `cfmakeraw` only
        // modifies in place through the exclusive reference.
        unsafe { libc::cfmakeraw(&mut raw) };
        raw.c_oflag = saved.c_oflag;
        let held = Self { fd, saved, raw };
        held.hold()?;
        Ok(held)
    }

    /// Applies raw mode again, in case a stop and resume undid it.
    pub(super) fn hold(&self) -> io::Result<()> {
        set(self.fd, &self.raw)
    }

    /// Restores the saved attributes and raises `SIGINT`, which Ctrl-C
    /// would have raised outside raw mode. Returns only if the signal is
    /// ignored or handled.
    pub(super) fn interrupt(&self) -> io::Error {
        let _ = set(self.fd, &self.saved);
        // SAFETY: `raise` takes no pointers; the attributes are restored
        // before the default action terminates the process.
        unsafe { libc::raise(libc::SIGINT) };
        io::Error::from(io::ErrorKind::Interrupted)
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        let _ = set(self.fd, &self.saved);
    }
}

fn get(fd: RawFd) -> io::Result<libc::termios> {
    let mut attributes = MaybeUninit::<libc::termios>::uninit();
    // SAFETY: the pointer is valid for writes of one `termios`, which
    // `tcgetattr` fills completely when it returns zero.
    if unsafe { libc::tcgetattr(fd, attributes.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `tcgetattr` succeeded, so every field is initialized.
    Ok(unsafe { attributes.assume_init() })
}

/// Applies `attributes` after pending output is written, without discarding
/// pending input, and retries when a signal interrupts the wait.
fn set(fd: RawFd, attributes: &libc::termios) -> io::Result<()> {
    loop {
        // SAFETY: `attributes` points to an initialized `termios` that
        // `tcsetattr` only reads.
        if unsafe { libc::tcsetattr(fd, libc::TCSADRAIN, attributes) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
