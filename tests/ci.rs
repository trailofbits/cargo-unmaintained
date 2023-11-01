use assert_cmd::Command;
use regex::Regex;
use similar_asserts::SimpleDiff;
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

#[cfg_attr(target_os = "macos", ignore)]
#[test]
fn format() {
    Command::new("rustup")
        .args(["run", "nightly", "cargo", "fmt", "--check"])
        .assert()
        .success();
}

#[test]
fn hack_feature_powerset() {
    Command::new("cargo")
        .env("RUSTFLAGS", "-D warnings")
        .args(["hack", "--feature-powerset", "check"])
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
        if [
            "AGPLv3 (1): cargo-unmaintained",
            "Custom License File (1): ring",
            "MPL-2.0 (1): uluru",
        ]
        .contains(&line)
        {
            continue;
        }
        assert!(re.is_match(line), "{line:?} does not match");
    }
}

#[cfg_attr(target_os = "windows", ignore)]
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

#[cfg_attr(target_os = "windows", ignore)]
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

    assert!(
        readme.contains(&usage),
        "{}",
        SimpleDiff::from_str(&readme, &usage, "left", "right")
    );
}

#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn sort() {
    Command::new("cargo")
        .args(["sort", "--check"])
        .assert()
        .success();
}
