#![cfg(feature = "test-ei")]

use std::process::Command;
use testing::{Tee, tee};

#[test]
fn dogfood() {
    let mut command = Command::new("cargo");
    command.args([
        "run",
        "--bin=cargo-unmaintained",
        "--",
        "unmaintained",
        "--color=never",
    ]);
    command.env_remove("CARGO_TERM_COLOR");

    let output = tee(command, Tee::Stdout).unwrap();

    assert!(output.status.success());
    assert_eq!(output.captured, [] as [u8; 0]);
}
