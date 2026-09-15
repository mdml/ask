//! Offline release archive safety checks run through the normal verification gate.

#[test]
fn nightly_packaging_safety() {
    let output = std::process::Command::new("python3")
        .arg("scripts/nightly-release-test.py")
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
