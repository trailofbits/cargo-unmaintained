#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]

use anyhow::{ensure, Result};
use snapbox::cmd::cargo_bin;
use std::{fs::OpenOptions, io::Write, path::Path, process::Command};
use tempfile::tempdir;

#[test]
fn ignored() -> Result<()> {
    let tempdir = tempdir()?;

    let status = Command::new("cargo")
        .args(["init", "--name=test-package"])
        .current_dir(&tempdir)
        .status()?;
    ensure!(status.success());

    let mut manifest = OpenOptions::new()
        .append(true)
        .open(tempdir.path().join("Cargo.toml"))?;
    writeln!(manifest, r#"lz4-compress = "*""#)?;

    let status = cargo_unmaintained(tempdir.path()).status()?;
    ensure!(!status.success());

    writeln!(
        manifest,
        r#"
[workspace.metadata.unmaintained]
ignored = ["lz4-compress"]
"#
    )?;

    let status = cargo_unmaintained(tempdir.path()).status()?;
    ensure!(status.success());

    Ok(())
}

fn cargo_unmaintained(dir: &Path) -> Command {
    let mut command = Command::new(cargo_bin("cargo-unmaintained"));
    command
        .args(["unmaintained", "--fail-fast"])
        .current_dir(dir);
    command
}
