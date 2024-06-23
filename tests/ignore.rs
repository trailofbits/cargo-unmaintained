#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]

use anyhow::{ensure, Result};
use snapbox::cmd::cargo_bin;
use std::{fs::OpenOptions, io::Write, path::Path, process::Command};
use tempfile::{tempdir, TempDir};

const NAME: &str = "bigint";

#[test]
fn ignore() -> Result<()> {
    let tempdir = create_test_package()?;

    add_dependency(tempdir.path(), NAME)?;

    let status = cargo_unmaintained(tempdir.path()).status()?;
    ensure!(!status.success());

    ignore_package(tempdir.path(), NAME)?;

    let status = cargo_unmaintained(tempdir.path()).status()?;
    ensure!(status.success());

    Ok(())
}

#[test]
fn warn_not_depended_upon() -> Result<()> {
    let tempdir = create_test_package()?;

    ignore_package(tempdir.path(), NAME)?;

    let output = cargo_unmaintained(tempdir.path()).output()?;
    ensure!(output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.lines().any(|line| line
            == format!(
                "warning: workspace metadata says to ignore `{NAME}`, but workspace does not \
                 depend upon `{NAME}`"
            )),
        "{stderr}"
    );

    Ok(())
}

fn create_test_package() -> Result<TempDir> {
    let tempdir = tempdir()?;

    let status = Command::new("cargo")
        .args(["init", "--name=test-package"])
        .current_dir(&tempdir)
        .status()?;
    ensure!(status.success());

    Ok(tempdir)
}

fn add_dependency(dir: &Path, name: &str) -> Result<()> {
    let mut manifest = OpenOptions::new()
        .append(true)
        .open(dir.join("Cargo.toml"))?;
    writeln!(manifest, r#"{name} = "*""#)?;
    Ok(())
}

fn ignore_package(dir: &Path, name: &str) -> Result<()> {
    let mut manifest = OpenOptions::new()
        .append(true)
        .open(dir.join("Cargo.toml"))?;
    writeln!(
        manifest,
        r#"
[workspace.metadata.unmaintained]
ignore = ["{name}"]
"#
    )?;
    Ok(())
}

fn cargo_unmaintained(dir: &Path) -> Command {
    let mut command = Command::new(cargo_bin("cargo-unmaintained"));
    command
        .args(["unmaintained", "--fail-fast"])
        .current_dir(dir);
    command
}
