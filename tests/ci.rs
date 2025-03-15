use std::{env::var, process::Command};

#[test]
fn ci() {
    if var("CI").is_ok() {
        return;
    }

    let status = Command::new("cargo")
        .args(["test", "-p", "ci"])
        .status()
        .unwrap();
    assert!(status.success());
}
