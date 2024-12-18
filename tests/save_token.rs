use snapbox::cmd::cargo_bin;
use std::{
    env::{remove_var, var},
    io::{stderr, Write},
    process::{Command, Stdio},
};

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn save_token() {
    let Ok(github_token) = var("GITHUB_TOKEN") else {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `save_token` test as `GTIHUB_TOKEN` is unset"
        )
        .unwrap();
        return;
    };

    let mut command = Command::new(cargo_bin("cargo-unmaintained"));
    command.args(["unmaintained", "--save-token"]);
    command.stdin(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{github_token}").unwrap();
    let exit_status = child.wait().unwrap();
    assert!(exit_status.success());

    let mut command = Command::new(cargo_bin("cargo-unmaintained"));
    command
        .args(["unmaintained", "--color=never"])
        .env_remove("GITHUB_TOKEN")
        .current_dir("fixtures/archived");
    let output = command.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stdout,
        "adler (https://github.com/jonas-schievink/adler.git archived)\n"
    );
    assert_eq!(
        stderr,
        "Scanning 1 packages and their dependencies (pass --verbose for more information)\n"
    );
}
