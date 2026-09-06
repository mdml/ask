fn main() -> rusqlite::Result<()> {
    let mut db = rusqlite::Connection::open_in_memory()?;
    let tx = db.transaction()?;
    tx.execute_batch("CREATE TABLE smoke (id INTEGER PRIMARY KEY, value TEXT NOT NULL); PRAGMA user_version = 1;")?;
    tx.execute("INSERT INTO smoke(value) VALUES (?1)", ["sample"])?;
    tx.commit()?;
    let value: String = db.query_row("SELECT value FROM smoke WHERE id = 1", [], |r| r.get(0))?;
    assert_eq!(value, "sample");
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    assert_eq!(version, 1);
    println!("SQLite {}: transaction, parameter binding, readback, user_version passed", rusqlite::version());
    Ok(())
}
