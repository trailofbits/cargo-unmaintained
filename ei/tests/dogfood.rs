use elaborate::std::env::set_current_dir_wc;
use std::{env::remove_var, process::Command};
use testing::{Tee, tee};

#[ctor::ctor(unsafe)]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    let _ = set_current_dir_wc("..");
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
    ]);

    let output = tee(command, Tee::Stdout).unwrap();

    assert!(output.status.success());
    assert!(output.captured.is_empty());
}
