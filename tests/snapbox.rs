#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]

use anyhow::Result;
use rayon::prelude::*;
use snapbox::{
    assert_matches_path,
    cmd::{cargo_bin, Command},
};
use std::{
    ffi::OsStr,
    fs::{read_dir, read_to_string},
};
use tempfile::tempdir;

#[test]
fn snapbox() -> Result<()> {
    let mut read_dir = read_dir("tests/cases")?;

    let url_paths = read_dir.try_fold(Vec::new(), |mut url_paths, entry| {
        let entry = entry?;
        let path = entry.path();
        if path.extension() == Some(OsStr::new("url")) {
            url_paths.push(path);
        }
        Result::<_>::Ok(url_paths)
    })?;

    url_paths.into_par_iter().try_for_each(|input_path| {
        let stdout_path = input_path.with_extension("stdout");
        let stderr_path = input_path.with_extension("stderr");

        let raw = read_to_string(input_path)?;

        let url = raw.trim_end();

        let tempdir = tempdir()?;

        Command::new("git")
            .args(["clone", "--depth=1", url, &tempdir.path().to_string_lossy()])
            .assert()
            .success();

        let output = Command::new(cargo_bin("cargo-unmaintained"))
            .args(["unmaintained"])
            .current_dir(&tempdir)
            .output()?;

        let stdout_actual = String::from_utf8(output.stdout)?;
        let stderr_actual = String::from_utf8(output.stderr)?;

        assert_matches_path(stdout_path, stdout_actual);
        assert_matches_path(stderr_path, stderr_actual);

        Result::<_>::Ok(())
    })?;

    Ok(())
}
