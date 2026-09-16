//! Offline live-provider check helper tests run through the normal verification gate.

#[test]
fn live_provider_check_support_is_offline() {
    let output = std::process::Command::new("python3")
        .arg("scripts/live-provider-check-test.py")
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
