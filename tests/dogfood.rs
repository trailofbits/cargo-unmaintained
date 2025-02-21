use snapbox::cmd::cargo_bin;
use std::{env::remove_var, process::Command};

mod util;
use util::{Tee, tee};

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn dogfood() {
    let mut command = Command::new(cargo_bin("cargo-unmaintained"));
    command.args(["unmaintained", "--color=never", "--verbose"]);

    let output = tee(command, Tee::Stdout).unwrap();

    assert!(output.status.success());
    assert!(output.captured.is_empty());
}
