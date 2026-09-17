use std::process::Command;

#[test]
fn offline_owner_workflow() {
    let output = Command::new("python3")
        .arg("scripts/offline-acceptance.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .output()
        .expect("run offline acceptance script");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
