use snapbox::{assert_data_eq, Data};
use std::{
    env::{remove_var, var},
    fs::write,
    path::PathBuf,
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
    let path_stdout = format!("tests/rustsec_issues.{}.stdout", token_modifier());

    let mut command = Command::new("cargo");
    command
        .args(["run", "--bin=rustsec_issues"])
        .current_dir("rustsec_util");

    let output = tee(command, Tee::Stdout).unwrap();

    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    if var("BLESS").is_ok() {
        write(path_stdout, stdout_actual).unwrap();
    } else {
        assert_data_eq!(
            stdout_actual,
            Data::read_from(&PathBuf::from(path_stdout), None),
        );
    }
}
