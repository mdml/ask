use rusqlite::{Connection, ErrorCode, TransactionBehavior, params};
use std::{
    io::{self, BufRead},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const SOURCE: &str =
    "2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn count(db: &Connection) -> Result<i64> {
    Ok(db.query_row("SELECT count(*) FROM records", [], |r| r.get(0))?)
}

fn contender(path: &Path) -> Result<()> {
    let mut db = Connection::open(path)?;
    db.busy_timeout(Duration::from_millis(200))?;
    let start = Instant::now();
    let error = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap_err();
    assert_eq!(error.sqlite_error_code(), Some(ErrorCode::DatabaseBusy));
    assert!(start.elapsed() >= Duration::from_millis(150));
    assert_eq!(count(&db)?, 1);
    println!("busy-confirmed");
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    assert_eq!(line.trim(), "retry");
    db.execute("INSERT INTO records(value) VALUES (?1)", ["contender"])?;
    Ok(())
}

fn exercise(path: &Path, mode: &str) -> Result<()> {
    let mut db = Connection::open(path)?;
    let actual: String = db.query_row(&format!("PRAGMA journal_mode={mode}"), [], |r| r.get(0))?;
    assert_eq!(actual.to_uppercase(), mode);
    db.execute_batch("CREATE TABLE records(id INTEGER PRIMARY KEY, value TEXT NOT NULL)")?;
    let value = "synthetic ' bound parameter";
    let tx = db.transaction()?;
    tx.execute("INSERT INTO records(value) VALUES (?1)", params![value])?;
    tx.commit()?;
    let tx = db.transaction()?;
    tx.execute("INSERT INTO records(value) VALUES ('rollback')", [])?;
    tx.rollback()?;
    {
        let tx = db.transaction()?;
        tx.execute("INSERT INTO records(value) VALUES ('drop rollback')", [])?;
    }
    drop(db);
    let mut db = Connection::open(path)?;
    assert_eq!(count(&db)?, 1);
    let stored: String = db.query_row("SELECT value FROM records", [], |r| r.get(0))?;
    assert_eq!(stored, value);
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("INSERT INTO records(value) VALUES ('uncommitted')", [])?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("contender")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut line = String::new();
    io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line)?;
    assert_eq!(line.trim(), "busy-confirmed");
    tx.rollback()?;
    use std::io::Write;
    writeln!(child.stdin.take().unwrap(), "retry")?;
    assert!(child.wait()?.success());
    assert_eq!(count(&db)?, 2);
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    assert_eq!(integrity, "ok");
    drop(db);
    assert_eq!(count(&Connection::open(path)?)?, 2);
    println!(
        "{mode}: commit/reopen, explicit/drop rollback, cross-process busy timeout, reader isolation, retry, integrity: PASS"
    );
    Ok(())
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "contender") {
        return contender(Path::new(&args[2]));
    }
    assert_eq!(args.len(), 2, "supply an empty probe directory");
    let root = Path::new(&args[1]);
    assert!(root.is_dir() && root.read_dir()?.next().is_none());
    let db = Connection::open(root.join("native.db"))?;
    assert_eq!(rusqlite::version(), "3.53.2");
    let source: String = db.query_row("SELECT sqlite_source_id()", [], |r| r.get(0))?;
    assert_eq!(source, SOURCE);
    println!("sqlite_version={} source_id={source}", rusqlite::version());
    let mut options = db.prepare("PRAGMA compile_options")?;
    for option in options.query_map([], |r| r.get::<_, String>(0))? {
        println!("compile_option={}", option?);
    }
    exercise(&root.join("delete.db"), "DELETE")?;
    exercise(&root.join("wal.db"), "WAL")?;
    Ok(())
}

fn main() {
    if run().is_err() {
        eprintln!("SQLite readiness probe failed; no database contents or paths logged");
        std::process::exit(1);
    }
}
