#![cfg(test)]

use assert_cmd::assert::OutputAssertExt;
use regex::Regex;
use similar_asserts::SimpleDiff;
use std::{
    env::{remove_var, set_current_dir, var},
    ffi::OsStr,
    fs::{read_to_string, write},
    ops::Range,
    path::Path,
    process::{Command, ExitStatus},
    str::FromStr,
};
use tempfile::tempdir;
use testing::split_at_cut_lines;
use walkdir::WalkDir;

#[ctor::ctor]
fn initialize() {
    unsafe {
        remove_var("CARGO_TERM_COLOR");
    }
    set_current_dir("..");
}

#[test]
fn clippy() {
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
        .assert()
        .success();
}

#[test]
fn dylint() {
    let assert = Command::new("cargo")
        .args(["dylint", "--all", "--", "--all-features", "--all-targets"])
        .env("DYLINT_RUSTFLAGS", "--deny warnings")
        .assert();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(assert.try_success().is_ok(), "{}", stderr);
}

#[test]
fn elaborate_disallowed_methods() {
    elaborate::disallowed_methods()
        .args(["--all-features", "--all-targets"])
        .env("RUSTUP_TOOLCHAIN", "nightly")
        .assert()
        .success();
}

#[test]
fn fmt() {
    Command::new("rustup")
        .args(["run", "nightly", "cargo", "fmt", "--check"])
        .assert()
        .success();
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
            "--exclude=cache-repositories,ei",
            "udeps",
        ])
        .assert()
        .success();
}

#[test]
fn license() {
    let re = Regex::new(r"^[^:]*\b(Apache-2.0|BSD-3-Clause|CDLA-Permissive-2.0|ISC|MIT|Zlib)\b")
        .unwrap();

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
            == "Unicode-3.0 (18): icu_collections, icu_locale_core, icu_normalizer, \
                icu_normalizer_data, icu_properties, icu_properties_data, icu_provider, litemap, \
                potential_utf, tinystr, writeable, yoke, yoke-derive, zerofrom, zerofrom-derive, \
                zerotrie, zerovec, zerovec-derive"
        {
            continue;
        }
        assert!(re.is_match(line), "{line:?} does not match");
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
    let config = concat!(env!("CARGO_MANIFEST_DIR"), "/markdown_link_check.json");

    let readme_md = concat!(env!("CARGO_MANIFEST_DIR"), "/../README.md");

    Command::new("npx")
        .args(["markdown-link-check", "--config", config, readme_md])
        .current_dir(&tempdir)
        .assert()
        .success();
}

#[test]
fn mock_github_lockfile_is_up_to_date() {
    Command::new("cargo")
        .args(["check", "--locked"])
        .current_dir("mock_github")
        .assert()
        .success();
}

#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn prettier() {
    const ARGS: &[&str] = &[
        "{}/**/*.md",
        "{}/**/*.yml",
        "!{}/target/**",
        "!{}/mock_github/target/**",
    ];

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
    let contents = read_to_string("ei/tests/rustsec_advisories.stdout").unwrap();
    let (_, middle_expected, bottom_expected) = split_at_cut_lines(&contents).unwrap();

    let readme = read_to_string("README.md").unwrap();
    let lines = readme.lines();

    let mut lines = lines.skip_while(|&line| line != "<!-- as-of start -->");
    assert_eq!(lines.next(), Some("<!-- as-of start -->"));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), Some(middle_expected.trim_end()));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), Some("<!-- as-of end -->"));

    let mut lines = lines
        .skip_while(|&line| line != "<!-- not-identified start -->")
        .peekable();
    assert_eq!(lines.next(), Some("<!-- not-identified start -->"));
    assert_eq!(lines.next(), Some(""));
    let bottom_actual = lines
        .take_while(|&line| line != "<!-- not-identified end -->")
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    assert_eq!(bottom_expected.to_owned() + "\n", bottom_actual);
}

#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn readme_contains_usage() {
    let readme = read_to_string("README.md").unwrap();

    let assert = Command::new("cargo")
        .args([
            "run",
            "--bin=cargo-unmaintained",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"),
            "--",
            "unmaintained",
            "--help",
        ])
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

#[test]
fn dependencies_are_sorted() {
    for entry in WalkDir::new(".")
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_name() == OsStr::new("Cargo.toml"))
    {
        let path = entry.path();
        let contents = read_to_string(path).unwrap();
        let document = contents.parse::<toml_edit::Document<_>>().unwrap();
        for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(span) = key_value_pair_span(&document, table_name) else {
                continue;
            };
            assert!(
                key_value_pairs_are_sorted(&document, span),
                "`{table_name}` in `{}` are not sorted",
                path.display()
            );
        }
    }
}

fn key_value_pair_span<S>(
    document: &toml_edit::Document<S>,
    table_name: &str,
) -> Option<Range<usize>> {
    // smoelius: The table might not exist.
    let item = document.get(table_name)?;
    let table = item.as_table().unwrap();
    // smoelius: The table might exist but be empty.
    let (_, last_item) = table.iter().last()?;
    let header_span = table.span().unwrap();
    let last_item_span = last_item.span().unwrap();
    Some(header_span.end..last_item_span.end)
}

fn key_value_pairs_are_sorted<S: AsRef<str>>(
    document: &toml_edit::Document<S>,
    span: Range<usize>,
) -> bool {
    for group in groups(document, span) {
        let pairs = &document.raw()[group]
            .parse::<toml_edit::Document<_>>()
            .unwrap();
        if !pairs.iter().map(|(k, _)| k).is_sorted() {
            return false;
        }
    }
    true
}

fn groups<S: AsRef<str>>(
    document: &toml_edit::Document<S>,
    span: Range<usize>,
) -> Vec<Range<usize>> {
    let group_starts = group_starts(document, &span);
    let mut groups = Vec::with_capacity(group_starts.len() + 1);
    let mut start = span.start;
    for partition in group_starts {
        groups.push(start..partition);
        start = partition;
    }
    groups.push(start..span.end);
    groups
}

/// Find the offsets in `span` that are not newlines, but that are preceded by two (or more)
/// newlines.
fn group_starts<S: AsRef<str>>(
    document: &toml_edit::Document<S>,
    span: &Range<usize>,
) -> Vec<usize> {
    let raw = &document.raw()[span.clone()].as_bytes();
    (2..raw.len())
        .filter(|&i| raw[i - 2] == b'\n' && raw[i - 1] == b'\n' && raw[i] != b'\n')
        .map(|i| span.start + i)
        .collect()
}

#[cfg_attr(target_os = "windows", ignore)]
#[test]
fn supply_chain() {
    let mut command = Command::new("cargo");
    command.args(["supply-chain", "update", "--cache-max-age=0s"]);
    let _: ExitStatus = command.status().unwrap();

    let mut command = Command::new("cargo");
    command.args(["supply-chain", "json", "--no-dev"]);
    let assert = command.assert().success();

    let stdout_actual = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    let mut value = serde_json::Value::from_str(stdout_actual).unwrap();
    remove_avatars(&mut value);
    let stdout_normalized = serde_json::to_string_pretty(&value).unwrap();

    let path_buf = Path::new(env!("CARGO_MANIFEST_DIR")).join("supply_chain.json");

    if enabled("BLESS") {
        write(path_buf, stdout_normalized).unwrap();
    } else {
        let stdout_expected = read_to_string(&path_buf).unwrap();

        assert!(
            stdout_expected == stdout_normalized,
            "{}",
            SimpleDiff::from_str(&stdout_expected, &stdout_normalized, "left", "right")
        );
    }
}

fn remove_avatars(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
        serde_json::Value::Array(array) => {
            for value in array {
                remove_avatars(value);
            }
        }
        serde_json::Value::Object(object) => {
            object.retain(|key, value| {
                if key == "avatar" {
                    return false;
                }
                remove_avatars(value);
                true
            });
        }
    }
}

fn enabled(key: &str) -> bool {
    var(key).is_ok_and(|value| value != "0")
}
