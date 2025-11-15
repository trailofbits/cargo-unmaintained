#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]

use anyhow::{Result, ensure};
use assert_cmd::cargo;
use elaborate::std::{fs::OpenOptionsContext, process::CommandContext};
use std::{fs::OpenOptions, io::Write, path::Path, process::Command};
use tempfile::{TempDir, tempdir};

const NAME: &str = "bigint";

#[test]
fn ignore() -> Result<()> {
    let tempdir = create_test_package(None)?;

    add_dependency(tempdir.path(), NAME)?;

    let status = cargo_unmaintained(tempdir.path()).status_wc()?;
    ensure!(!status.success());

    ignore_package(tempdir.path(), NAME)?;

    let status = cargo_unmaintained(tempdir.path()).status_wc()?;
    ensure!(status.success());

    Ok(())
}

#[test]
fn warn_not_depended_upon() -> Result<()> {
    let tempdir = create_test_package(None)?;

    ignore_package(tempdir.path(), NAME)?;

    let output = cargo_unmaintained(tempdir.path()).output_wc()?;
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

fn create_test_package(name: Option<&str>) -> Result<TempDir> {
    let tempdir = tempdir()?;

    let status = Command::new("cargo")
        .args(["init", "--lib", "--name", name.unwrap_or("test-package")])
        .current_dir(&tempdir)
        .status_wc()?;
    ensure!(status.success());

    Ok(tempdir)
}

fn add_dependency(dir: &Path, name: &str) -> Result<()> {
    let mut manifest = OpenOptions::new()
        .append(true)
        .open_wc(dir.join("Cargo.toml"))?;
    writeln!(manifest, r#"{name} = "*""#)?;
    Ok(())
}

fn ignore_package(dir: &Path, name: &str) -> Result<()> {
    let mut manifest = OpenOptions::new()
        .append(true)
        .open_wc(dir.join("Cargo.toml"))?;
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
    #[cfg_attr(
        dylint_lib = "general",
        allow(abs_home_path, unnecessary_conversion_for_trait)
    )]
    // smoelius: `Command::new(cargo_bin!(..))` because this function's return type is
    // `std::process::Command`, not `assert_cmd::Command`.
    let mut command = Command::new(cargo::cargo_bin!("cargo-unmaintained"));
    command
        .args(["unmaintained", "--fail-fast"])
        .current_dir(dir);
    command
}

#[cfg(not(windows))]
mod not_windows {
    use super::*;
    use elaborate::std::{
        fs::{read_to_string_wc, write_wc},
        path::PathContext,
    };

    #[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
    #[test]
    fn renamed_default_branch() -> Result<()> {
        let cache_dir = tempdir().unwrap();
        let tempdir = create_test_package(None)?;
        let test_dependency = create_test_package(Some("dummy"))?;

        // smoelius: Hack.
        set_repository_to_self(test_dependency.path()).unwrap();

        add_local_dependency(tempdir.path(), "dummy", test_dependency.path())?;

        let status = cargo_unmaintained(tempdir.path())
            .arg("--max-age=0")
            .env("CARGO_UNMAINTAINED_CACHE", cache_dir.path())
            .status_wc()?;
        ensure!(!status.success());

        rename_master_to_main(test_dependency.path());

        let status = cargo_unmaintained(tempdir.path())
            .arg("--max-age=0")
            .env("CARGO_UNMAINTAINED_CACHE", cache_dir.path())
            .status_wc()?;
        ensure!(!status.success());

        assert_all_repositories_use_main(cache_dir.path());

        Ok(())
    }

    /// Hack. Set `repository = "{dir}"` for the Cargo.toml file in dir.
    fn set_repository_to_self(dir: &Path) -> Result<()> {
        let manifest_path = dir.join("Cargo.toml");
        let manifest = read_to_string_wc(&manifest_path)?;
        let mut lines = manifest.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
        let last = lines.pop().unwrap();
        assert_eq!("[dependencies]", last);
        lines.push(format!(r#"repository = "{}""#, dir.display()));
        lines.push(last);
        write_wc(
            manifest_path,
            lines
                .into_iter()
                .map(|line| format!("{line}\n"))
                .collect::<String>(),
        )?;
        Ok(())
    }

    fn add_local_dependency(dir: &Path, name: &str, path: &Path) -> Result<()> {
        let mut manifest = OpenOptions::new()
            .append(true)
            .open_wc(dir.join("Cargo.toml"))?;
        writeln!(manifest, r#"{name} = {{ path = "{}" }}"#, path.display())?;
        Ok(())
    }

    fn rename_master_to_main(path: &Path) {
        assert_eq!("master", branch_name(path));

        let mut command = Command::new("git");
        command.args(["branch", "-m", "master", "main"]);
        command.current_dir(path);
        let status = command.status_wc().unwrap();
        assert!(status.success());

        assert_eq!("main", branch_name(path));
    }

    fn assert_all_repositories_use_main(cache_dir: &Path) {
        let repositories = cache_dir.join("v2/repositories");
        let read_dir = repositories.read_dir_wc().unwrap();
        let results = read_dir.into_iter().collect::<Vec<_>>();
        assert_eq!(1, results.len());
        let entry = results[0].as_ref().unwrap();
        assert_eq!("main", branch_name(&entry.path()));
    }

    fn branch_name(dir: &Path) -> String {
        let mut command = Command::new("git");
        command.args(["branch", "--show-current"]);
        command.current_dir(dir);
        let output = command.output_wc().unwrap();
        assert!(output.status.success());
        let stdout = std::str::from_utf8(&output.stdout).unwrap();
        stdout.trim_end().to_owned()
    }
}
