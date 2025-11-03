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
    let mut command = Command::new("cargo");
    command.args([
        "run",
        "--bin=cargo-unmaintained",
        "--manifest-path",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"),
        "--",
        "unmaintained",
        "--color=never",
        "--verbose",
    ]);

    let output = tee(command, Tee::Stdout).unwrap();

    assert!(output.status.success());
    assert!(output.captured.is_empty());
}
