use elaborate::std::{
    env::{set_current_dir_wc, var_wc},
    fs::write_wc,
};
use snapbox::{Data, assert_data_eq};
use std::{env::remove_var, path::PathBuf, process::Command};
use testing::{Tee, tee};

#[ctor::ctor(unsafe)]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    let _ = set_current_dir_wc("..");
}

#[test]
fn rustsec_issues() {
    const PATH_STDOUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/rustsec_issues.stdout");

    let mut command = Command::new("cargo");
    command.args(["run", "--example=rustsec_issues"]);

    let output = tee(command, Tee::Stdout).unwrap();

    let stdout_actual = std::str::from_utf8(&output.captured).unwrap();

    if var_wc("BLESS").is_ok() {
        write_wc(PATH_STDOUT, stdout_actual).unwrap();
    } else {
        assert_data_eq!(
            stdout_actual,
            Data::read_from(&PathBuf::from(PATH_STDOUT), None),
        );
    }
}
