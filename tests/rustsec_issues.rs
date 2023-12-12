use snapbox::assert_matches_path;
use std::{
    env::{remove_var, var},
    process::Command,
};

mod util;
use util::{tee, token_modifier, Tee};

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn rustsec_issues() {
    // smoelius: If we are running on GitHub (i.e., `CI` is set), run only if `GITHUB_TOKEN_PATH` is
    // also set. This is to avoid hitting API rate limits.
    if var("CI").is_ok() && var("GITHUB_TOKEN_PATH").is_err() {
        return;
    }

    let mut command = Command::new("cargo");
    command
        .args(["run", "--bin=rustsec_issues"])
        .current_dir("rustsec_util");

    let output = tee(command, Tee::Stdout).unwrap();

    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    assert_matches_path(
        format!("tests/rustsec_issues.{}.stdout", token_modifier()),
        stdout_actual,
    );
}
