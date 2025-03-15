use snapbox::cmd::cargo_bin;
use std::{
    env::{remove_var, set_current_dir},
    process::Command,
};
use testing::{Tee, tee};

#[ctor::ctor]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    set_current_dir("..");
}

#[test]
fn dogfood() {
    let mut command = Command::new(cargo_bin("cargo-unmaintained"));
    command.args(["unmaintained", "--color=never", "--verbose"]);

    let output = tee(command, Tee::Stdout).unwrap();

    assert!(output.status.success());
    assert!(output.captured.is_empty());
}
