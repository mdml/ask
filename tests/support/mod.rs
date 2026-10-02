#![allow(dead_code)]

pub mod fake_provider;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

pub const CREDENTIAL: &str = "credential-secret-never-print";

/// Credential variables `ask init` presets read; proofs never inherit them.
pub const PRESET_VARIABLES: [&str; 7] = [
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "OPENROUTER_API_KEY",
    "GROQ_API_KEY",
    "CEREBRAS_API_KEY",
    "XAI_API_KEY",
];
/// Selects the published model list `ask init` offers without a key. Every
/// process the proofs start sets it, empty unless a case points it at a fake,
/// so no proof contacts the project's published list.
pub const MODEL_LIST_URL: &str = "ASK_MODEL_LIST_URL";
static HOME_ID: AtomicUsize = AtomicUsize::new(0);

/// Builds a command for the `ask` binary with `ASK_HOME` set to `home`, the
/// published model list disabled, and the credential variable either set to
/// `CREDENTIAL` or removed.
pub fn command(home: &Path, with_credential: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ask"));
    command
        .env("ASK_HOME", home)
        .env(MODEL_LIST_URL, "")
        .env_remove("LOCAL_API_KEY");
    for variable in PRESET_VARIABLES {
        command.env_remove(variable);
    }
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
