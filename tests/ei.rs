use elaborate::std::{env::var_wc, process::CommandContext};
use std::process::Command;

#[test]
fn ei() {
    if var_wc("CI").is_ok() {
        return;
    }

    let status = Command::new("cargo")
        .args(["test", "-p", "ei"])
        .status_wc()
        .unwrap();
    assert!(status.success());
}
