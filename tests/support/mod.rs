#![allow(dead_code)]

pub mod fake_provider;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

pub const CREDENTIAL: &str = "credential-secret-never-print";
static HOME_ID: AtomicUsize = AtomicUsize::new(0);

/// Builds a command for the `ask` binary with `ASK_HOME` set to `home` and the
/// credential variable either set to `CREDENTIAL` or removed.
pub fn command(home: &Path, with_credential: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ask"));
    command.env("ASK_HOME", home).env_remove("LOCAL_API_KEY");
    if with_credential {
        command.env("LOCAL_API_KEY", CREDENTIAL);
    }
    command
}

/// Creates an empty, unique `ASK_HOME` directory under the Cargo target directory.
pub fn fresh_home() -> PathBuf {
    let id = HOME_ID.fetch_add(1, Ordering::SeqCst);
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/proof-tests")
        .join(format!("{}-{id}", std::process::id()));
    fs::create_dir_all(&home).unwrap();
    home
}
