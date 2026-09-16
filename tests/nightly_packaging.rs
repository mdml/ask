//! Offline release archive and stable-release helper checks run through verification.

fn run_helper(script: &str) {
    let output = std::process::Command::new("python3")
        .arg(script)
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

#[test]
fn nightly_packaging_safety() {
    run_helper("scripts/nightly-release-test.py");
}

#[test]
fn stable_packaging_safety() {
    run_helper("scripts/stable-release-test.py");
}

#[test]
fn homebrew_formula_safety() {
    run_helper("scripts/homebrew-formula-test.py");
}
