use std::process::Command;

#[test]
fn terminal_demo_records_the_installed_workflow() {
    let output = Command::new("python3")
        .arg("scripts/demo-test.py")
        .arg(env!("CARGO_BIN_EXE_ask"))
        .output()
        .expect("run terminal demo proof");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
