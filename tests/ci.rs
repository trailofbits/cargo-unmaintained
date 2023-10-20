use assert_cmd::Command;
use regex::Regex;
use std::{env::remove_var, fs::read_to_string};
use tempfile::tempdir;

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn clippy() {
    Command::new("cargo")
        .args([
            "clippy",
            "--all-features",
            "--all-targets",
            "--",
            "--deny=warnings",
            "--warn=clippy::pedantic",
        ])
        .assert()
        .success();
}

#[test]
fn dylint() {
    Command::new("cargo")
        .args(["dylint", "--all", "--", "--all-features", "--all-targets"])
        .env("DYLINT_RUSTFLAGS", "--deny warnings")
        .assert()
        .success();
}

#[test]
fn format() {
    Command::new("cargo")
        .args(["+nightly", "fmt", "--check"])
        .assert()
        .success();
}

#[test]
fn license() {
    let re = Regex::new(r"^[^:]*\b(Apache-2.0|BSD-3-Clause|ISC|MIT)\b").unwrap();

    for line in std::str::from_utf8(
        &Command::new("cargo")
            .arg("license")
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .unwrap()
    .lines()
    {
        if line == "Custom License File (1): ring" || line == "MPL-2.0 (1): uluru" {
            continue;
        }
        assert!(re.is_match(line), "{line:?} does not match");
    }
}

#[test]
fn prettier() {
    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "prettier"])
        .current_dir(&tempdir)
        .assert()
        .success();

    Command::new("npx")
        .args([
            "prettier",
            "--check",
            &format!("{}/**/*.md", env!("CARGO_MANIFEST_DIR")),
            &format!("{}/**/*.yml", env!("CARGO_MANIFEST_DIR")),
            &format!("!{}/target/**", env!("CARGO_MANIFEST_DIR")),
        ])
        .current_dir(&tempdir)
        .assert()
        .success();
}

#[test]
fn readme_contains_usage() {
    let readme = read_to_string("README.md").unwrap();

    let assert = Command::cargo_bin("cargo-unmaintained")
        .unwrap()
        .args(["unmaintained", "--help"])
        .assert();
    let stdout = &assert.get_output().stdout;

    let usage = std::str::from_utf8(stdout)
        .unwrap()
        .split_inclusive('\n')
        .skip(2)
        .collect::<String>();

    assert!(readme.contains(&usage));
}

#[test]
fn sort() {
    Command::new("cargo")
        .args(["sort", "--check"])
        .assert()
        .success();
}
