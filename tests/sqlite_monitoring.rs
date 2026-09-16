//! Offline tests of the report-only native SQLite monitor, run through the
//! normal verification gate. The monitor's upstream reads are fixtures here.

#[test]
fn sqlite_monitor_offline() {
    let output = std::process::Command::new("python3")
        .arg("scripts/sqlite-monitor-test.py")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("python3 is required by repository verification");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
