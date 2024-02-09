use anyhow::{ensure, Context, Result};
use std::{fs::OpenOptions, io::Write, process::Command};
use tempfile::{tempdir, TempDir};

pub(crate) fn temp_package(name: &str) -> Result<TempDir> {
    let tempdir = tempdir().with_context(|| "failed to create temporary directory")?;

    // smoelius: Passing `--vcs=none` adds a tiny bit of speedup. This is useful when `cargo
    // unmaintained` is called repeatedly, e.g., in the `rustsec_advisories` test.
    let status = Command::new("cargo")
        .args([
            "init",
            &format!("--name={name}-temp-package"),
            "--quiet",
            "--vcs=none",
        ])
        .current_dir(&tempdir)
        .status()
        .with_context(|| "failed to create temporary package")?;
    ensure!(status.success());

    let path = tempdir.path().join("Cargo.toml");
    let mut manifest = OpenOptions::new()
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {path:?}"))?;
    writeln!(manifest, r#"{name} = "*""#)
        .with_context(|| format!("failed to write to {path:?}"))?;

    Ok(tempdir)
}
