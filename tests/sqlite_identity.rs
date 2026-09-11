//! Native identity of the linked SQLite accepted in
//! `docs/reviews/sqlite-adoption-2026-09-10.md`: libsqlite3-sys 0.38.2 bundles
//! the official 3.53.2 amalgamation with this source id.

use rusqlite::Connection;

const VERSION: &str = "3.53.2";
const VERSION_NUMBER: i32 = 3_053_002;
const SOURCE_ID: &str =
    "2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24";

#[test]
fn linked_sqlite_is_the_accepted_amalgamation() {
    assert_eq!(rusqlite::version(), VERSION);
    assert_eq!(rusqlite::version_number(), VERSION_NUMBER);
    let db = Connection::open_in_memory().unwrap();
    let source_id: String = db
        .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(source_id, SOURCE_ID);
}
