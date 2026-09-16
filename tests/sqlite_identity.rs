//! Native identity of the linked SQLite against the ledger
//! `monitoring/sqlite-native.toml`, which records the version accepted in
//! `docs/reviews/sqlite-adoption-2026-09-10.md`: libsqlite3-sys 0.38.2 bundles
//! the official 3.53.2 amalgamation with this source id and these compile
//! options. A dependency update must update the ledger in the same change.

use rusqlite::Connection;
use toml::{Table, Value};

const LEDGER: &str = include_str!("../monitoring/sqlite-native.toml");

fn native() -> Table {
    let ledger: Table = toml::from_str(LEDGER).unwrap();
    ledger["native"].as_table().unwrap().clone()
}

#[test]
fn linked_sqlite_is_the_accepted_amalgamation() {
    let native = native();
    assert_eq!(
        rusqlite::version(),
        native["sqlite_version"].as_str().unwrap()
    );
    assert_eq!(
        i64::from(rusqlite::version_number()),
        native["sqlite_version_number"].as_integer().unwrap()
    );
    let db = Connection::open_in_memory().unwrap();
    let source_id: String = db
        .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(source_id, native["source_id"].as_str().unwrap());
}

/// The compile options are the same on every supported target except the
/// `COMPILER` entry, which names the host compiler.
#[test]
fn linked_sqlite_has_the_recorded_compile_options() {
    let db = Connection::open_in_memory().unwrap();
    let mut statement = db.prepare("PRAGMA compile_options").unwrap();
    let mut options: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .filter(|option: &String| !option.starts_with("COMPILER="))
        .collect();
    options.sort();
    let mut recorded: Vec<String> = native()["compile_options"]
        .as_array()
        .unwrap()
        .iter()
        .map(Value::as_str)
        .map(|option| option.unwrap().to_string())
        .collect();
    recorded.sort();
    assert_eq!(options, recorded);
}
