use snapbox::{Data, assert_data_eq};
use std::{
    env::{remove_var, var},
    fs::write,
    path::PathBuf,
    process::Command,
};

mod util;
use util::{Tee, tee};

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn rustsec_issues() {
    const PATH_STDOUT: &str = "tests/rustsec_issues.stdout";

    let mut command = Command::new("cargo");
    command.args(["run", "--example=rustsec_issues"]);

    let output = tee(command, Tee::Stdout).unwrap();

    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    if var("BLESS").is_ok() {
        write(PATH_STDOUT, stdout_actual).unwrap();
    } else {
        assert_data_eq!(
            stdout_actual,
            Data::read_from(&PathBuf::from(PATH_STDOUT), None),
        );
    }
}
