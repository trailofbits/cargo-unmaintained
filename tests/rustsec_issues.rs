#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]

use anyhow::{ensure, Result};
use log::debug;
use once_cell::sync::Lazy;
use regex::Regex;
use snapbox::{assert_matches_path, cmd::cargo_bin};
use std::{
    collections::HashSet,
    fmt::Write as _,
    fs::OpenOptions,
    io::Write as _,
    path::Path,
    process::{Command, ExitStatus},
};
use tempfile::tempdir;
use tokio::runtime;

macro_rules! tee {
    ($dst:expr, $fmt:expr, $($arg:tt)*) => {{
        eprintln!($fmt, $($arg)*);
        writeln!($dst, $fmt, $($arg)*)
    }};
}

#[allow(dead_code)]
#[derive(Debug)]
struct Output {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

static RT: Lazy<runtime::Runtime> = Lazy::new(|| {
    runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
});

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
#[test]
fn rustsec_issues() {
    let page = RT
        .block_on(async {
            let octocrab = octocrab::instance();
            octocrab
                .issues("rustsec", "advisory-db")
                .list()
                .state(octocrab::params::State::Open)
                .per_page(100)
                .send()
                .await
        })
        .unwrap();

    let mut issue_urls = page
        .items
        .iter()
        .filter_map(|issue| {
            if !issue.title.contains("unmaintained")
                && !issue
                    .labels
                    .iter()
                    .any(|label| label.name == "Unmaintained")
            {
                return None;
            };
            let mut urls = issue.body.as_deref().map(extract_urls).unwrap_or_default();
            if urls.is_empty() {
                return None;
            }
            urls.sort_unstable();
            urls.dedup();
            Some((issue.number, urls))
        })
        .collect::<Vec<_>>();

    issue_urls.sort();

    let mut stdout = String::new();

    for (number, urls) in issue_urls {
        tee!(
            stdout,
            "https://github.com/rustsec/advisory-db/issues/{}",
            number
        )
        .unwrap();
        let mut checked = HashSet::new();
        for url in urls {
            if let Some(name) = extract_package_name(url) {
                if checked.contains(name) {
                    continue;
                }
                checked.insert(name);
                tee!(
                    stdout,
                    "    {} `{name}`",
                    if is_unmaintained(name).unwrap() {
                        "found"
                    } else {
                        "failed to find"
                    }
                )
                .unwrap();
            } else {
                tee!(stdout, "    ignoring `{}`", url).unwrap();
            }
        }
    }

    assert_matches_path("tests/rustsec_issues.stdout", stdout);
}

static URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\bhttps://[^\s()<>]*").unwrap());

fn extract_urls(body: &str) -> Vec<&str> {
    URL_RE.find_iter(body).map(|m| m.as_str()).collect()
}

static NAME_RES: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"^https://crates\.io/(crates/)?(?<name>[0-9A-Za-z_-]*)",
        r"^https://docs.rs/(?<name>[0-9A-Za-z_-]*)",
        r"^https://github\.com/[0-9A-Za-z_-]*/(?<name>[0-9A-Za-z_-]*)",
        r"^https://lib\.rs/crates/(?<name>[0-9A-Za-z_-]*)",
        r"^https://sourcegraph\.com/crates/(?<name>[0-9A-Za-z_-]*)",
    ]
    .into_iter()
    .map(|re| Regex::new(re).unwrap())
    .collect()
});

fn extract_package_name(url: &str) -> Option<&str> {
    NAME_RES
        .iter()
        .find_map(|re| re.captures(url))
        .map(|captures| captures.name("name").unwrap().as_str())
        .filter(|name| !["advisory-db", "cargo", "rust"].contains(name))
}

fn is_unmaintained(name: &str) -> Result<bool> {
    let tempdir = tempdir()?;

    let output = command_output(
        Command::new("cargo")
            .args(["init", &format!("--name={name}-test-package")])
            .current_dir(&tempdir),
    )?;
    ensure!(output.status.success(), "{:#?}", output);

    let mut manifest = OpenOptions::new()
        .append(true)
        .open(tempdir.path().join("Cargo.toml"))?;
    writeln!(manifest, r#"{name} = "*""#)?;

    let output = command_output(&mut cargo_unmaintained(name, tempdir.path()))?;

    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => {
            debug!("{output:#?}");
            Ok(false)
        }
    }
}

fn cargo_unmaintained(name: &str, dir: &Path) -> Command {
    let mut command = Command::new(cargo_bin("cargo-unmaintained"));
    command
        .args(["unmaintained", "--fail-fast", "-p", name])
        .current_dir(dir);
    command
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn command_output(command: &mut Command) -> Result<Output> {
    let output = command.output()?;
    let status = output.status;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
