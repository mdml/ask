//! The damaged-file differential from `docs/reviews/sqlite-adoption-2026-09-10.md`,
//! run against the linked SQLite on every supported target: one index-cell
//! payload length inflated from 4 to 100 on a 512-byte page. A version without
//! check-in 6826c17021 (the linked 3.53.2) returns the row from an indexed
//! `SELECT`; a version with the fix returns `SQLITE_CORRUPT`. The ledger states
//! which behavior is expected, so a version change forces a deliberate update.

use std::{fs, path::PathBuf};

use rusqlite::{Connection, ErrorCode};
use toml::Table;

const PAGE_SIZE: usize = 512;
const LEDGER: &str = include_str!("../monitoring/sqlite-native.toml");
const INDEXED_SELECT: &str = "SELECT v FROM t INDEXED BY ix WHERE v = 'a'";

fn scratch(name: &str) -> PathBuf {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/damaged-file-tests")
        .join(std::process::id().to_string());
    fs::create_dir_all(&directory).unwrap();
    directory.join(name)
}

/// One text row at rowid 1 and one index: the index-leaf record for 'a' plus
/// the rowid key has payload length 4.
fn pristine_image() -> Vec<u8> {
    let path = scratch("pristine.db");
    let _ = fs::remove_file(&path);
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "PRAGMA page_size = 512; PRAGMA journal_mode = DELETE; CREATE TABLE t(v TEXT); INSERT INTO t(rowid, v) VALUES (1, 'a'); CREATE INDEX ix ON t(v);",
    )
    .unwrap();
    assert_eq!(integrity(&db), "ok");
    drop(db);
    fs::read(path).unwrap()
}

/// Inflates the first index-leaf cell's payload-length byte from 4 to 100.
fn corrupt_index_cell(image: &mut [u8]) {
    assert_eq!(image.len() % PAGE_SIZE, 0);
    for page in 0..image.len() / PAGE_SIZE {
        let base = page * PAGE_SIZE;
        let header = base + if page == 0 { 100 } else { 0 };
        let cells = u16::from_be_bytes([image[header + 3], image[header + 4]]);
        if image[header] != 0x0A || cells == 0 {
            continue;
        }
        let pointer = u16::from_be_bytes([image[header + 8], image[header + 9]]);
        let target = base + usize::from(pointer);
        assert_eq!(image[target], 4, "unexpected index-cell payload length");
        image[target] = 100;
        return;
    }
    panic!("no index-leaf cell located");
}

fn integrity(db: &Connection) -> String {
    db.query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn damaged_index_cell_behaves_as_the_ledger_records() {
    let ledger: Table = toml::from_str(LEDGER).unwrap();
    let fix_present = ledger["native"]["index_bounds_fix_6826c17021_present"]
        .as_bool()
        .unwrap();
    let mut image = pristine_image();
    corrupt_index_cell(&mut image);
    let path = scratch("damaged.db");
    fs::write(&path, &image).unwrap();
    let db = Connection::open(&path).unwrap();
    let selected = db.query_row(INDEXED_SELECT, [], |row| row.get::<_, String>(0));
    if fix_present {
        let error = selected.unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(ErrorCode::DatabaseCorrupt),
            "{error}"
        );
    } else {
        assert_eq!(selected.unwrap(), "a");
    }
    let report = integrity(&db);
    assert_ne!(report, "ok");
    assert!(
        report
            .chars()
            .all(|character| !character.is_control() || character == '\n'),
        "{report}"
    );
    assert_eq!(fs::read(&path).unwrap(), image, "the check must not write");
}
