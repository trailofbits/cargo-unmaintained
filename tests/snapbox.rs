#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]

use anyhow::{bail, Context, Result};
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use serde::Deserialize;
use snapbox::{
    assert_data_eq,
    cmd::{cargo_bin, Command as SnapboxCommand},
    Data,
};
use std::{
    env::var,
    ffi::OsStr,
    fs::{read_dir, read_to_string},
    io::{stderr, Write},
    path::{Path, PathBuf},
    process::Command,
};

mod util;
use util::{enabled, tee, Tee};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Test {
    /// Repo path (cannot be used in conjunction with `url`)
    path: Option<String>,

    /// Repo url (cannot be used in conjunction with `path`)
    url: Option<String>,

    /// Repo revision; `None` (the default) means the head of the default branch
    #[serde(default)]
    rev: Option<String>,
}

#[cfg_attr(dylint_lib = "supplementary", allow(commented_code))]
#[test]
fn snapbox() -> Result<()> {
    // #[cfg(not(feature = "lock-index"))]
    // panic!("the `snapbox` test requires the `lock-index` feature");

    let test_cases = Path::new("tests/cases");

    let test_paths = if let Ok(testname) = var("TESTNAME") {
        vec![test_cases.join(testname).with_extension("toml")]
    } else {
        let mut read_dir = read_dir(test_cases)?;

        read_dir.try_fold(Vec::new(), |mut url_paths, entry| {
            let entry = entry?;
            let path = entry.path();
            if path.extension() == Some(OsStr::new("toml")) {
                url_paths.push(path);
            }
            Result::<_>::Ok(url_paths)
        })?
    };

    test_paths
        .into_par_iter()
        .panic_fuse()
        .try_for_each(|input_path| {
            let stderr_path = input_path.with_extension("stderr");
            let stdout_path = input_path.with_extension("stdout");

            let raw = read_to_string(input_path)?;

            let test: Test = toml::from_str(&raw).unwrap();

            // smoelius: I learned this conditional initialization trick from Solana's source code:
            // https://github.com/solana-labs/rbpf/blob/f52bfa0f4912d5f6eaa364de7c42b6ee6be50a88/src/elf.rs#L401
            let tempdir: tempfile::TempDir;
            let dir = match (test.path, test.url) {
                (Some(path), None) => PathBuf::from(path),
                (None, Some(url)) => {
                    tempdir = tempfile::tempdir()?;

                    // smoelius: Perform the checkout as a separate step so that errors that occur
                    // in it can be ignored.
                    let mut command = SnapboxCommand::new("git").args([
                        "clone",
                        "--no-checkout",
                        &url,
                        &tempdir.path().to_string_lossy(),
                    ]);
                    if test.rev.is_none() {
                        command = command.arg("--depth=1");
                    }
                    command.assert().success();

                    checkout(tempdir.path(), test.rev.as_deref()).unwrap();

                    tempdir.path().to_owned()
                }
                (_, _) => {
                    panic!("exactly one of `path` and `url` must be set");
                }
            };

            let path_buf = dir.join("Cargo.lock");
            assert!(path_buf.exists(), "{path_buf:?} does not exist");

            let mut command = Command::new(cargo_bin("cargo-unmaintained"));
            command
                .args(["unmaintained", "--color=never"])
                .current_dir(dir);

            if enabled("VERBOSE") {
                // smoelius If `VERBOSE` is enabled, don't bother comparing stderr, because it won't
                // match.
                command.arg("--verbose");

                let output = tee(command, Tee::Stdout)?;

                let stdout_actual = String::from_utf8(output.captured)?;

                assert_data_eq!(stdout_actual, Data::read_from(&stdout_path, None));
            } else {
                let output = command.output()?;

                let stderr_actual = String::from_utf8(output.stderr)?;
                let stdout_actual = String::from_utf8(output.stdout)?;

                // smoelius: Compare stderr before stdout so that you can see any errors that
                // occurred.
                assert_data_eq!(stderr_actual, Data::read_from(&stderr_path, None));
                assert_data_eq!(stdout_actual, Data::read_from(&stdout_path, None));
            }

            Ok(())
        })
}

static GIT_CONFIG: Lazy<tempfile::NamedTempFile> = Lazy::new(|| {
    let mut tempfile = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        tempfile,
        "\
[core]
        protectNTFS = false"
    )
    .unwrap();
    tempfile
});

fn checkout(repo_dir: &Path, rev: Option<&str>) -> Result<()> {
    for second_attempt in [false, true] {
        let mut command = Command::new("git");
        command.args(["checkout", "--quiet"]);
        if let Some(rev) = rev {
            command.arg(rev);
        }
        if second_attempt {
            command.env("GIT_CONFIG_GLOBAL", GIT_CONFIG.path());
        }
        command.current_dir(repo_dir);
        let output = command
            .output()
            .with_context(|| format!("failed to run command: {command:?}"))?;
        if !output.status.success() {
            let error = String::from_utf8(output.stderr)?;
            let msg = format!(
                "failed to checkout `{}`: ```\n{}```",
                repo_dir.display(),
                error
            );
            if second_attempt {
                bail!(msg);
            }
            #[allow(clippy::explicit_write)]
            writeln!(stderr(), "{msg}\nretrying with `GIT_CONFIG_GLOBAL`").unwrap();
        }
    }
    Ok(())
}

static RES: Lazy<[Regex; 2]> = Lazy::new(|| {
    [
        Regex::new(r"([^ ]*) days").unwrap(),
        Regex::new(r"latest: ([^ )]*)").unwrap(),
    ]
});

#[test]
fn snapbox_expected() -> Result<()> {
    for entry in read_dir("tests/cases")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("stdout")) {
            continue;
        }
        let contents = read_to_string(path)?;
        for line in contents.lines() {
            for re in &*RES {
                if let Some(captures) = re.captures(line) {
                    assert_eq!(2, captures.len());
                    assert_eq!("[..]", &captures[1]);
                }
            }
        }
    }

    Ok(())
}
