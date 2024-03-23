use snapbox::{assert_matches, Data};
use std::{env::remove_var, path::PathBuf, process::Command};

mod util;
use util::{tee, token_modifier, Tee};

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn rustsec_issues() {
    let mut command = Command::new("cargo");
    command
        .args(["run", "--bin=rustsec_issues"])
        .current_dir("rustsec_util");

    let output = tee(command, Tee::Stdout).unwrap();

    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    assert_matches(
        Data::read_from(
            &PathBuf::from(format!("tests/rustsec_issues.{}.stdout", token_modifier())),
            None,
        ),
        stdout_actual,
    );
}
