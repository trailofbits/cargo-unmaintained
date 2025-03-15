use snapbox::{Data, assert_data_eq};
use std::{
    env::{remove_var, set_current_dir, var},
    fs::write,
    path::PathBuf,
    process::Command,
};
use testing::{Tee, tee};

#[ctor::ctor]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    set_current_dir("..");
}

#[test]
fn rustsec_issues() {
    const PATH_STDOUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/rustsec_issues.stdout");

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
