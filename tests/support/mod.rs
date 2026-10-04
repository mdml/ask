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
/// Proxy variables the HTTP client reads. Every process the proofs start
/// removes them and then sets `NO_PROXY` to `LOOPBACK_HOSTS`, so requests to a
/// loopback fake go direct even under an ambient proxy or, on macOS, a system
/// proxy setting, which only `NO_PROXY` exempts.
pub const PROXY_VARIABLES: [&str; 8] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];
pub const LOOPBACK_HOSTS: &str = "127.0.0.1,localhost";
static HOME_ID: AtomicUsize = AtomicUsize::new(0);

/// Builds a command for the `ask` binary with `ASK_HOME` set to `home`, the
/// published model list and proxies disabled, and the credential variable
/// either set to `CREDENTIAL` or removed.
pub fn command(home: &Path, with_credential: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ask"));
    isolate(&mut command, home);
    for variable in PRESET_VARIABLES {
        command.env_remove(variable);
    }
    if with_credential {
        command.env("LOCAL_API_KEY", CREDENTIAL);
    }
    command
}

/// Runs the binary under a one-block file-size limit so any write beyond the
/// first block fails. `SIGXFSZ` is ignored so the child reports the error itself.
#[cfg(unix)]
pub fn write_limited(home: &Path, arguments: &[&str]) -> Command {
    let mut limited = Command::new("sh");
    limited
        .args([
            "-c",
            "trap '' XFSZ; ulimit -f 1; exec \"$@\"",
            "write-limit",
            env!("CARGO_BIN_EXE_ask"),
        ])
        .args(arguments)
        // The disk limit would also truncate this child's coverage profile.
        .env("LLVM_PROFILE_FILE", "/dev/null");
    isolate(&mut limited, home);
    limited
}

/// Sets `ASK_HOME`, disables the published list and proxies, and removes the
/// fake provider's credential variable.
fn isolate(command: &mut Command, home: &Path) {
    command
        .env("ASK_HOME", home)
        .env(MODEL_LIST_URL, "")
        .env_remove("LOCAL_API_KEY");
    for variable in PROXY_VARIABLES {
        command.env_remove(variable);
    }
    command.env("NO_PROXY", LOOPBACK_HOSTS);
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
