//! Noninteractive checking and application of a complete configuration document.
//!
//! `check` validates a candidate and writes nothing. `apply` validates the same
//! candidate with the same validator and then installs its exact bytes. Neither
//! reads a credential value nor contacts a provider.

use std::{
    fs,
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{cli::Source, validate};

/// Why publishing a configuration file failed.
#[derive(Debug)]
pub enum WriteError {
    Exists,
    Failed(String),
}

/// Validates a candidate document without touching the installed configuration.
pub fn check(source: &Source, input: &mut impl Read) -> Result<String, String> {
    let candidate = read(source, input)?;
    validate::document(&candidate).map_err(|problem| invalid(source, &problem))?;
    Ok(format!("{source} is a valid configuration"))
}

/// Validates a candidate document and installs its exact bytes at `destination`.
pub fn apply(destination: &Path, source: &Source, input: &mut impl Read) -> Result<String, String> {
    let _lock = publication_lock(destination).map_err(|error| cannot_write(destination, &error))?;
    let before = current(destination)?;
    let candidate = read(source, input)?;
    validate::document(&candidate).map_err(|problem| invalid(source, &problem))?;
    let replaced = install(destination, candidate.as_bytes(), before)?;
    let verb = if replaced { "replaced" } else { "created" };
    Ok(format!("{verb} '{}'", destination.display()))
}

fn invalid(source: &Source, problem: &str) -> String {
    format!("{source} is not a valid configuration: {problem}")
}

fn read(source: &Source, input: &mut impl Read) -> Result<String, String> {
    match source {
        Source::Stdin => {
            let mut candidate = String::new();
            input
                .read_to_string(&mut candidate)
                .map_err(|error| format!("cannot read standard input: {error}"))?;
            Ok(candidate)
        }
        Source::File(path) => fs::read_to_string(path)
            .map_err(|error| format!("cannot read '{}': {error}", path.display())),
    }
}

/// Holds the reusable lock inode until publication finishes. Never unlink it:
/// another writer may already have opened that same inode.
fn publication_lock(destination: &Path) -> Result<fs::File, WriteError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    let path = destination.with_file_name(".ask-config.lock");
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(SAFE_OPEN)
        .open(path)
        .map_err(failed)?;
    regular(&file.metadata().map_err(failed)?).map_err(failed)?;
    file.try_lock()
        .map_err(|_| WriteError::Failed("configuration publication lock is unavailable".into()))?;
    Ok(file)
}

// O_NOFOLLOW | O_NONBLOCK: reject symlinks and never block on a substituted FIFO.
#[cfg(target_os = "linux")]
const SAFE_OPEN: i32 = 0x20000 | 0x800;
#[cfg(target_os = "macos")]
const SAFE_OPEN: i32 = 0x100 | 0x4;

struct Snapshot {
    bytes: Vec<u8>,
    metadata: fs::Metadata,
}

fn regular(metadata: &fs::Metadata) -> io::Result<()> {
    if !metadata.is_file() {
        return Err(io::Error::other(
            "configuration destination must be a regular file, not a symlink",
        ));
    }
    Ok(())
}

fn current(destination: &Path) -> Result<Option<Snapshot>, String> {
    snapshot(destination)
        .map_err(|error| format!("cannot inspect '{}': {error}", destination.display()))
}

fn snapshot(destination: &Path) -> io::Result<Option<Snapshot>> {
    let metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    regular(&metadata)?;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(SAFE_OPEN)
        .open(destination)?;
    let metadata = file.metadata()?;
    regular(&metadata)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Some(Snapshot { bytes, metadata }))
}

fn same(before: Option<&Snapshot>, after: Option<&Snapshot>) -> bool {
    match (before, after) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.bytes == b.bytes
                && a.metadata.dev() == b.metadata.dev()
                && a.metadata.ino() == b.metadata.ino()
                && a.metadata.mode() == b.metadata.mode()
                && a.metadata.ctime() == b.metadata.ctime()
                && a.metadata.ctime_nsec() == b.metadata.ctime_nsec()
        }
        _ => false,
    }
}

/// Stage exact bytes, check the snapshot, then atomically publish under the lock.
/// External editors ignoring the lock can race the check and rename; this is
/// deliberately not an operating-system compare-and-swap guarantee.
fn install(destination: &Path, contents: &[u8], before: Option<Snapshot>) -> Result<bool, String> {
    let temporary = staged(destination, contents, before.as_ref())
        .map_err(|error| cannot_write(destination, &error))?;
    replace(destination, &temporary.path, before.as_ref())?;
    Ok(before.is_some())
}

fn replace(destination: &Path, temporary: &Path, before: Option<&Snapshot>) -> Result<(), String> {
    if !same(before, current(destination)?.as_ref()) {
        return Err(format!(
            "'{}' changed while it was being replaced; nothing was written",
            destination.display()
        ));
    }
    publish(destination, temporary, before.is_some())
        .map_err(|error| cannot_write(destination, &error))
}

fn publish(destination: &Path, temporary: &Path, replacing: bool) -> Result<(), WriteError> {
    let result = if replacing {
        fs::rename(temporary, destination)
    } else {
        fs::hard_link(temporary, destination)
    };
    result.map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => WriteError::Exists,
        _ => failed(error),
    })
}

/// Initialization participates in the same lock and never replaces a destination.
pub(crate) fn create_new(destination: &Path, contents: &[u8]) -> Result<(), WriteError> {
    let _lock = publication_lock(destination)?;
    if fs::symlink_metadata(destination).is_ok() {
        return Err(WriteError::Exists);
    }
    let temporary = staged(destination, contents, None)?;
    publish(destination, &temporary.path, false)
}

fn staged(
    destination: &Path,
    contents: &[u8],
    before: Option<&Snapshot>,
) -> Result<TemporaryConfig, WriteError> {
    let mut temporary = TemporaryConfig::create(destination).map_err(failed)?;
    temporary.file.write_all(contents).map_err(failed)?;
    if let Some(snapshot) = before {
        temporary
            .file
            .set_permissions(fs::Permissions::from_mode(snapshot.metadata.mode() & 0o777))
            .map_err(failed)?;
    }
    temporary.file.sync_all().map_err(failed)?;
    Ok(temporary)
}

fn failed(error: io::Error) -> WriteError {
    WriteError::Failed(error.to_string())
}

pub(crate) fn cannot_write(destination: &Path, error: &WriteError) -> String {
    match error {
        WriteError::Exists => format!(
            "configuration already exists at '{}'",
            destination.display()
        ),
        WriteError::Failed(detail) => {
            format!("cannot write '{}': {detail}", destination.display())
        }
    }
}

struct TemporaryConfig {
    path: PathBuf,
    file: fs::File,
}

impl TemporaryConfig {
    fn create(destination: &Path) -> io::Result<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                destination.with_file_name(format!(".ask-config-{}-{id}.tmp", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for TemporaryConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "configure_tests.rs"]
mod tests;
