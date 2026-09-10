use std::{
    io::Cursor,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;

static CASE_ID: AtomicUsize = AtomicUsize::new(0);

const CANDIDATE: &str = r#"# a comment the exact bytes must keep
default_profile = "default"

[providers.local]
kind = "openai-compatible"
base_url   =    "http://127.0.0.1:1234/v1"
api_key_env = "LOCAL_API_KEY"

[profiles.default]
provider = "local"
model = "fake-model"
"#;

fn fresh_path() -> PathBuf {
    let id = CASE_ID.fetch_add(1, Ordering::SeqCst);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/configure-unit-tests")
        .join(format!("{}-{id}", std::process::id()))
        .join("config.toml")
}

fn stdin(text: &str) -> Cursor<Vec<u8>> {
    Cursor::new(text.as_bytes().to_vec())
}

#[test]
fn check_accepts_a_valid_candidate_and_writes_nothing() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "installed = true\n").unwrap();
    let message = check(&Source::Stdin, &mut stdin(CANDIDATE)).unwrap();
    assert_eq!(message, "standard input is a valid configuration");
    assert_eq!(fs::read_to_string(&path).unwrap(), "installed = true\n");
    let entries = fs::read_dir(path.parent().unwrap()).unwrap().count();
    assert_eq!(entries, 1);
}

#[test]
fn check_reports_an_invalid_candidate_by_source() {
    let message = check(&Source::Stdin, &mut stdin("default_profile = 1\n")).unwrap_err();
    assert!(
        message.starts_with("standard input is not a valid configuration: "),
        "{message}"
    );
}

#[test]
fn a_named_file_is_read_and_a_missing_one_is_reported() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let candidate = path.with_file_name("candidate.toml");
    fs::write(&candidate, CANDIDATE).unwrap();
    check(&Source::File(candidate), &mut stdin("")).unwrap();
    let absent = path.with_file_name("absent.toml");
    let message = check(&Source::File(absent), &mut stdin("")).unwrap_err();
    assert!(message.starts_with("cannot read '"), "{message}");
}

#[test]
fn apply_creates_then_replaces_and_preserves_exact_bytes() {
    let path = fresh_path();
    let created = apply(&path, &Source::Stdin, &mut stdin(CANDIDATE)).unwrap();
    assert_eq!(created, format!("created '{}'", path.display()));
    assert_eq!(fs::read(&path).unwrap(), CANDIDATE.as_bytes());

    let replacement = CANDIDATE.replace("fake-model", "other-model");
    let replaced = apply(&path, &Source::Stdin, &mut stdin(&replacement)).unwrap();
    assert_eq!(replaced, format!("replaced '{}'", path.display()));
    assert_eq!(fs::read(&path).unwrap(), replacement.as_bytes());
}

#[test]
fn apply_refuses_an_invalid_candidate_and_leaves_the_destination_alone() {
    let path = fresh_path();
    apply(&path, &Source::Stdin, &mut stdin(CANDIDATE)).unwrap();
    let message = apply(
        &path,
        &Source::Stdin,
        &mut stdin(&CANDIDATE.replace("model =", "modle =")),
    )
    .unwrap_err();
    assert!(message.contains("not a valid configuration"), "{message}");
    assert_eq!(fs::read(&path).unwrap(), CANDIDATE.as_bytes());
}

#[test]
fn apply_leaves_no_temporary_file_behind() {
    let path = fresh_path();
    apply(&path, &Source::Stdin, &mut stdin(CANDIDATE)).unwrap();
    let entries: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries.len(), 2);
    assert!(entries.contains(&std::ffi::OsString::from(".ask-config.lock")));
}

#[test]
fn a_destination_that_changes_during_apply_is_not_replaced() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "what apply read earlier\n").unwrap();
    let before = current(&path).unwrap();
    fs::write(&path, "a concurrent writer got here first\n").unwrap();
    let temporary = path.with_file_name("candidate.tmp");
    fs::write(&temporary, CANDIDATE).unwrap();
    let message = replace(&path, &temporary, before.as_ref()).unwrap_err();
    assert_eq!(
        message,
        format!(
            "'{}' changed while it was being replaced; nothing was written",
            path.display()
        )
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "a concurrent writer got here first\n"
    );
    assert_eq!(fs::read_to_string(&temporary).unwrap(), CANDIDATE);
}

#[test]
fn an_unreadable_destination_is_reported_before_anything_is_staged() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "").unwrap();
    let nested = path.join("config.toml");
    let message = apply(&nested, &Source::Stdin, &mut stdin(CANDIDATE)).unwrap_err();
    assert!(message.starts_with("cannot write '"), "{message}");
}

#[cfg(unix)]
#[test]
fn a_failed_write_reports_the_destination_and_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let path = fresh_path();
    let directory = path.parent().unwrap();
    fs::create_dir_all(directory).unwrap();
    fs::set_permissions(directory, fs::Permissions::from_mode(0o555)).unwrap();
    let message = apply(&path, &Source::Stdin, &mut stdin(CANDIDATE)).unwrap_err();
    fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        message.starts_with(&format!("cannot write '{}': ", path.display())),
        "{message}"
    );
    assert_eq!(fs::read_dir(directory).unwrap().count(), 0);
}

#[test]
fn create_new_refuses_an_existing_destination() {
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "original").unwrap();
    assert!(matches!(create_new(&path, b"new"), Err(WriteError::Exists)));
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    assert_eq!(
        cannot_write(&path, &WriteError::Exists),
        format!("configuration already exists at '{}'", path.display())
    );
    assert_eq!(
        cannot_write(&path, &WriteError::Failed("boom".to_string())),
        format!("cannot write '{}': boom", path.display())
    );
}

#[test]
fn snapshot_precedes_candidate_read_and_checks_identity() {
    struct Editor {
        path: PathBuf,
        input: Cursor<Vec<u8>>,
    }
    impl std::io::Read for Editor {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.input.position() == 0 {
                let other = self.path.with_extension("other");
                fs::write(&other, CANDIDATE)?;
                fs::rename(other, &self.path)?;
            }
            self.input.read(buf)
        }
    }
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, CANDIDATE).unwrap();
    let mut editor = Editor {
        path: path.clone(),
        input: stdin(CANDIDATE),
    };
    assert!(
        apply(&path, &Source::Stdin, &mut editor)
            .unwrap_err()
            .contains("changed")
    );
}

#[test]
fn unsafe_destinations_are_refused_without_changing_their_targets() {
    use std::os::unix::fs::symlink;
    let path = fresh_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let target = path.with_extension("target");
    fs::write(&target, "original").unwrap();
    symlink(&target, &path).unwrap();
    assert!(apply(&path, &Source::Stdin, &mut stdin(CANDIDATE)).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"original");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(apply(&path, &Source::Stdin, &mut stdin(CANDIDATE)).is_err());
}

#[test]
fn permissions_and_reusable_lock_inode_survive_publication() {
    let path = fresh_path();
    apply(&path, &Source::Stdin, &mut stdin(CANDIDATE)).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    let lock = path.with_file_name(".ask-config.lock");
    let inode = fs::metadata(&lock).unwrap().ino();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    apply(&path, &Source::Stdin, &mut stdin(CANDIDATE)).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o640);
    assert_eq!(fs::metadata(&lock).unwrap().ino(), inode);
}
