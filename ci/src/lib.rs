use assert_cmd::Command;
use regex::Regex;
use similar_asserts::SimpleDiff;
use std::{
    env::{remove_var, set_current_dir},
    fs::read_to_string,
    path::Path,
};
use tempfile::tempdir;
use testing::split_at_cut_line;

static DIRS: &[&str] = &["."];

#[ctor::ctor]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    set_current_dir("..");
}

#[test]
fn clippy() {
    for dir in DIRS {
        Command::new("cargo")
            // smoelius: Remove `CARGO` environment variable to work around:
            // https://github.com/rust-lang/rust/pull/131729
            .env_remove("CARGO")
            .args([
                "+nightly",
                "clippy",
                "--all-features",
                "--all-targets",
                "--",
                "--deny=warnings",
            ])
            .current_dir(dir)
            .assert()
            .success();
    }
}

#[test]
fn dylint() {
    for dir in DIRS {
        let assert = Command::new("cargo")
            .args(["dylint", "--all", "--", "--all-features", "--all-targets"])
            .env("DYLINT_RUSTFLAGS", "--deny warnings")
            .current_dir(dir)
            .assert();
        let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        assert!(assert.try_success().is_ok(), "{}", stderr);
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
    for dir in DIRS {
        Command::new("rustup")
            .env("RUSTFLAGS", "-D warnings")
            .args([
                "run",
                "nightly",
                "cargo",
                "hack",
                "--feature-powerset",
                "--exclude=cache-repositories,ei",
                "udeps",
            ])
            .current_dir(dir)
            .assert()
            .success();
    }
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
                "N/A (1): testing",
            ]
            .contains(&line)
            {
                continue;
            }
            // smoelius: Exception for `idna` dependencies.
            if line
                == "Unicode-3.0 (19): icu_collections, icu_locid, icu_locid_transform, \
                    icu_locid_transform_data, icu_normalizer, icu_normalizer_data, icu_properties, \
                    icu_properties_data, icu_provider, icu_provider_macros, litemap, tinystr, \
                    writeable, yoke, yoke-derive, zerofrom, zerofrom-derive, zerovec, \
                    zerovec-derive"
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

    // smoelius: Pin `markdown-link-check` to version 3.11 until the following issue is resolved:
    // https://github.com/tcort/markdown-link-check/issues/304
    Command::new("npm")
        .args(["install", "markdown-link-check@3.11"])
        .current_dir(&tempdir)
        .assert()
        .success();

    // smoelius: https://github.com/rust-lang/crates.io/issues/788
    let config = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/markdown_link_check.json"
    );

    let readme_md = concat!(env!("CARGO_MANIFEST_DIR"), "/../README.md");

    Command::new("npx")
        .args(["markdown-link-check", "--config", config, readme_md])
        .current_dir(&tempdir)
        .assert()
        .success();
}

#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn prettier() {
    const ARGS: &[&str] = &["{}/**/*.md", "{}/**/*.yml", "!{}/target/**"];

    // smoelius: Copied from Necessist:
    // Prettier's handling of `..` seems to have changed between versions 3.4 and 3.5.
    // Manually collapsing the `..` avoids the problem.
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "prettier"])
        .current_dir(&tempdir)
        .assert()
        .success();

    Command::new("npx")
        .args(["prettier", "--check"])
        .args(
            ARGS.iter()
                .map(|s| s.replace("{}", &parent.to_string_lossy())),
        )
        .current_dir(&tempdir)
        .assert()
        .success();
}

#[test]
fn readme_contains_expected_contents() {
    let readme = read_to_string("README.md").unwrap();
    let contents = read_to_string("ei/tests/rustsec_advisories.stdout").unwrap();
    let expected_contents = below_cut_line(&contents).unwrap();
    for expected_line in expected_contents.lines() {
        assert!(
            readme.lines().any(|line| line == expected_line),
            "failed to find line:\n```\n{expected_line}\n```",
        );
    }
}

fn below_cut_line(s: &str) -> Option<&str> {
    split_at_cut_line(s).map(|(_, below)| below)
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

#[test]
fn readme_reference_links_are_used() {
    let re = Regex::new(r"(?m)^(\[[^\]]*\]):").unwrap();
    let readme = read_to_string("README.md").unwrap();
    for captures in re.captures_iter(&readme) {
        assert_eq!(2, captures.len());
        let m = captures.get(1).unwrap();
        assert!(
            readme[..m.start()].contains(m.as_str()),
            "{} is unused",
            m.as_str()
        );
    }
}

#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn sort() {
    for dir in DIRS {
        Command::new("cargo")
            .args(["sort", "--check", "--no-format"])
            .current_dir(dir)
            .assert()
            .success();
    }
}
