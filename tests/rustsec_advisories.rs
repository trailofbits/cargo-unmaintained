use snapbox::assert_data_eq;
use std::{env::remove_var, fs::read_to_string, process::Command};

mod util;
use util::{split_at_cut_line, tee, Tee};

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

    let stdout_expected = read_to_string("tests/rustsec_advisories.stdout").unwrap();
    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    assert_data_eq!(
        above_cut_line(stdout_actual),
        above_cut_line(&stdout_expected),
    );
}

fn above_cut_line(s: &str) -> &str {
    split_at_cut_line(s).map_or(s, |(above, _)| above)
}
