use std::{env::var, process::Command};

#[test]
fn ei() {
    if var("CI").is_ok() {
        return;
    }

    let status = Command::new("cargo")
        .args(["test", "-p", "ei"])
        .status()
        .unwrap();
    assert!(status.success());
}
