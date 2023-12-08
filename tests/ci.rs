use assert_cmd::Command;
use regex::Regex;
use similar_asserts::SimpleDiff;
use std::{env::remove_var, fs::read_to_string, path::Path};
use tempfile::tempdir;

static DIRS: &[&str] = &[".", "rustsec_util"];

#[ctor::ctor]
fn initialize() {
    remove_var("CARGO_TERM_COLOR");
}

#[test]
fn clippy() {
    for dir in DIRS {
        Command::new("cargo")
            .args([
                "clippy",
                "--all-features",
                "--all-targets",
                "--",
                "--deny=warnings",
                "--warn=clippy::pedantic",
                "--allow=clippy::format-collect",
                "--allow=clippy::missing-errors-doc",
                "--allow=clippy::missing-panics-doc",
            ])
            .current_dir(dir)
            .assert()
            .success();
    }
}

#[test]
fn dylint() {
    for dir in DIRS {
        Command::new("cargo")
            .args(["dylint", "--all", "--", "--all-features", "--all-targets"])
            .env("DYLINT_RUSTFLAGS", "--deny warnings")
            .current_dir(dir)
            .assert()
            .success();
    }
}

#[cfg_attr(target_os = "macos", ignore)]
#[test]
fn format() {
    for dir in DIRS {
        Command::new("rustup")
            .args(["run", "nightly", "cargo", "fmt", "--check"])
            .current_dir(dir)
            .assert()
            .success();
    }
}

#[test]
fn hack_feature_powerset_udeps() {
    Command::new("rustup")
        .env("RUSTFLAGS", "-D warnings")
        .args([
            "run",
            "nightly",
            "cargo",
            "hack",
            "--feature-powerset",
            "udeps",
        ])
        .assert()
        .success();
}

#[test]
fn license() {
    let re = Regex::new(r"^[^:]*\b(Apache-2.0|BSD-3-Clause|ISC|MIT)\b").unwrap();

    for dir in DIRS {
        for line in std::str::from_utf8(
            &Command::new("cargo")
                .arg("license")
                .current_dir(dir)
                .assert()
                .success()
                .get_output()
                .stdout,
        )
        .unwrap()
        .lines()
        {
            if [
                "AGPL-3.0 (1): cargo-unmaintained",
                "AGPL-3.0 (1): rustsec_util",
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
}

#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn markdown_link_check() {
    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "markdown-link-check"])
        .current_dir(&tempdir)
        .assert()
        .success();

    // smoelius: https://github.com/rust-lang/crates.io/issues/788
    let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/markdown_link_check.json");

    let readme_md = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");

    Command::new("npx")
        .args([
            "markdown-link-check",
            "--config",
            &config.to_string_lossy(),
            &readme_md.to_string_lossy(),
        ])
        .current_dir(&tempdir)
        .assert()
        .success();
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

#[test]
fn readme_reference_links_are_sorted() {
    let re = Regex::new(r"^\[[^\]]*\]:").unwrap();
    let readme = read_to_string("README.md").unwrap();
    let links = readme
        .lines()
        .filter(|line| re.is_match(line))
        .collect::<Vec<_>>();
    let mut links_sorted = links.clone();
    links_sorted.sort_unstable();
    assert_eq!(links_sorted, links);
}

#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn sort() {
    for dir in DIRS {
        Command::new("cargo")
            .args(["sort", "--check"])
            .current_dir(dir)
            .assert()
            .success();
    }
}
