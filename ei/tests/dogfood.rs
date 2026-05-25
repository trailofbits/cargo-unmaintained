use std::process::Command;
use testing::{Tee, tee};

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
    command.env_remove("CARGO_TERM_COLOR");
    command.current_dir("..");

    let output = tee(command, Tee::Stdout).unwrap();

    assert!(output.status.success());
    assert!(output.captured.is_empty());
}
