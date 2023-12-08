use snapbox::assert_matches_path;
use std::{env::remove_var, process::Command};

mod util;
use util::{tee, token_modifier, Tee};

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn rustsec_advisories() {
    let mut command = Command::new("cargo");
    command
        .args(["run", "--bin=rustsec_advisories"])
        .current_dir("rustsec_util")
        .env("RUST_BACKTRACE", "0");

    let output = tee(command, Tee::Stdout).unwrap();

    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    assert_matches_path(
        format!("tests/rustsec_advisories.{}.stdout", token_modifier()),
        stdout_actual,
    );
}
