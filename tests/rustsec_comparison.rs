use snapbox::{assert_matches_path, cmd::Command};
use std::env::remove_var;

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn rustsec_comparison() {
    let assert = Command::new("cargo")
        .arg("run")
        .current_dir("rustsec_comparison")
        .env("RUST_BACKTRACE", "0")
        .assert();

    let stdout_actual = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    assert_matches_path("tests/rustsec_comparison.stdout", stdout_actual);
}
