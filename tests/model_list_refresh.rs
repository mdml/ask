//! Offline published model list refresh tests run through the normal verification gate.

#[test]
fn model_list_refresh_support_is_offline() {
    let output = std::process::Command::new("python3")
        .arg("scripts/model-list-refresh-test.py")
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
