use snapbox::assert_data_eq;
use std::{
    env::{remove_var, var},
    fs::{read_to_string, write},
    io::{stderr, Write},
    process::Command,
};

mod util;
use util::{split_at_cut_line, tee, Tee};

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn rustsec_advisories() {
    const PATH_STDOUT: &str = "tests/rustsec_advisories.stdout";

    let mut command = Command::new("cargo");
    command
        .args(["run", "--example=rustsec_advisories"])
        .env("RUST_BACKTRACE", "0");

    let output = tee(command, Tee::Stdout).unwrap();

    let stdout_expected = read_to_string(PATH_STDOUT).unwrap();
    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    if var("BLESS").is_ok() {
        write(PATH_STDOUT, stdout_actual).unwrap();

        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "`{PATH_STDOUT}` was overwritten and may need to be adjusted."
        )
        .unwrap();
    } else {
        assert_data_eq!(
            above_cut_line(stdout_actual),
            above_cut_line(&stdout_expected),
        );
    }
}

fn above_cut_line(s: &str) -> &str {
    split_at_cut_line(s).map_or(s, |(above, _)| above)
}
