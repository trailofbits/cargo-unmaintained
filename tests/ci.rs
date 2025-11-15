use elaborate::std::{env::var_wc, process::CommandContext};
use std::process::Command;

#[test]
fn ci() {
    if var_wc("CI").is_ok() {
        return;
    }

    let status = Command::new("cargo")
        .args(["test", "-p", "ci"])
        .status_wc()
        .unwrap();
    assert!(status.success());
}
