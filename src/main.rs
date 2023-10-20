#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use anyhow::{anyhow, ensure, Context, Result};
use cargo_metadata::{
    semver::Version, Dependency, DependencyKind, Metadata, MetadataCommand, Package,
};
use clap::{crate_version, Parser};
use crates_index::GitIndex;
use log::debug;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    cell::RefCell,
    collections::HashMap,
    env::args,
    path::Path,
    process::{exit, Command, Stdio},
    str::FromStr,
    time::SystemTime,
};
use tempfile::tempdir;

mod opts;

const SECS_PER_DAY: u64 = 24 * 60 * 60;

#[derive(Debug, Parser)]
#[clap(bin_name = "cargo", display_name = "cargo")]
struct Cargo {
    #[clap(subcommand)]
    subcmd: CargoSubCommand,
}

#[derive(Debug, Parser)]
enum CargoSubCommand {
    Unmaintained(Opts),
}

#[derive(Debug, Parser)]
#[clap(
    version = crate_version!(),
    after_help = "\
Unless --no-exit-code is passed, the exit status is 0 if-and-only-if no unmaintained dependencies \
were found and no irrecoverable errors occurred."
)]
struct Opts {
    #[clap(long, help = "Show paths to dependencies")]
    tree: bool,

    #[clap(
        long,
        help = "Do not set exit status when unmaintained dependencies are found"
    )]
    no_exit_code: bool,

    #[clap(long, help = "Suppress warnings")]
    quiet: bool,

    #[clap(
        long,
        help = "Age in days that a repository's last commit must not exceed for the repository to \
                be considered current",
        default_value = "365"
    )]
    max_age: u64,
}

struct UnmaintainedPkg<'a> {
    pkg: &'a Package,
    url_and_age: Option<(&'a str, u64)>,
    outdated_deps: Vec<OutdatedDep<'a>>,
}

struct OutdatedDep<'a> {
    dep: &'a Dependency,
    version_used: &'a Version,
    version_latest: Version,
}

thread_local! {
    #[allow(clippy::unwrap_used)]
    static INDEX: Lazy<GitIndex> = Lazy::new(|| GitIndex::new_cargo_default().unwrap());
    static LATEST_VERSION_CACHE: RefCell<Option<HashMap<String, Version>>> = RefCell::new(None);
    static TIMESTAMP_CACHE: RefCell<Option<HashMap<String, u64>>> = RefCell::new(None);
}

macro_rules! warn {
    ($format:literal, $($arg:expr),*) => {
        if !crate::opts::get().quiet {
            eprintln!(concat!("warning: ", $format), $($arg),*);
        }
    };
}

fn main() -> Result<()> {
    env_logger::init();

    let Cargo {
        subcmd: CargoSubCommand::Unmaintained(opts),
    } = Cargo::parse_from(args());

    opts::init(opts);

    let metadata = MetadataCommand::new().exec()?;

    let mut unnmaintained_pkgs = Vec::new();

    for pkg in &metadata.packages {
        let upgradeable_deps = outdated_deps(&metadata, pkg)?;

        if upgradeable_deps.is_empty() {
            continue;
        }

        let url_and_age = latest_commit_age(pkg)?;

        if url_and_age
            .as_ref()
            .map_or(false, |&(_, age)| age < opts::get().max_age * SECS_PER_DAY)
        {
            continue;
        }

        unnmaintained_pkgs.push(UnmaintainedPkg {
            pkg,
            url_and_age,
            outdated_deps: upgradeable_deps,
        });
    }

    unnmaintained_pkgs
        .sort_by_key(|unmaintained| unmaintained.url_and_age.as_ref().map(|&(_, age)| age));

    for unmaintained_pkg in &unnmaintained_pkgs {
        display_unmaintained_pkg(unmaintained_pkg)?;
    }

    if !opts::get().no_exit_code && !unnmaintained_pkgs.is_empty() {
        exit(1);
    }

    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
fn outdated_deps<'a>(metadata: &'a Metadata, pkg: &'a Package) -> Result<Vec<OutdatedDep<'a>>> {
    if !published(pkg) {
        return Ok(Vec::new());
    }
    let mut deps = Vec::new();
    for dep in &pkg.dependencies {
        // smoelius: Don't check dependencies specified by path.
        if dep.path.is_some() {
            continue;
        }
        let Some(dep_pkg) = find_package(metadata, dep) else {
            debug!(
                "warning: failed to find `{}` dependency `{}` ({})",
                pkg.name,
                dep.name,
                dep.req.to_string()
            );
            debug_assert!(dep.kind == DependencyKind::Development || dep.optional);
            continue;
        };
        let Ok(version_latest) = latest_version(&dep.name).map_err(|error| {
            warn!("failed to get latest version of `{}`: {}", &dep.name, error);
            debug_assert!(false);
        }) else {
            continue;
        };
        if dep_pkg.version <= version_latest && !dep.req.matches(&version_latest) {
            deps.push(OutdatedDep {
                dep,
                version_used: &dep_pkg.version,
                version_latest,
            });
        };
    }
    Ok(deps)
}

fn find_package<'a>(metadata: &'a Metadata, dep: &Dependency) -> Option<&'a Package> {
    metadata
        .packages
        .iter()
        .find(|pkg| dep.name == pkg.name && dep.req.matches(&pkg.version))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn latest_version(name: &str) -> Result<Version> {
    LATEST_VERSION_CACHE.with_borrow_mut(|latest_version_cache| {
        let latest_version_cache = latest_version_cache.get_or_insert_with(HashMap::new);
        if let Some(version) = latest_version_cache.get(name) {
            return Ok(version.clone());
        }
        let krate = INDEX
            .with(|index| index.crate_(name))
            .ok_or_else(|| anyhow!("failed to find `{}` in index", name))?;
        let latest_version_index = krate
            .highest_normal_version()
            .ok_or_else(|| anyhow!("`{}` has no normal version", name))?;
        let latest_version = Version::from_str(latest_version_index.version())?;
        latest_version_cache.insert(name.to_owned(), latest_version.clone());
        Ok(latest_version)
    })
}

fn published(pkg: &Package) -> bool {
    pkg.publish
        .as_ref()
        .map_or(true, |registries| !registries.is_empty())
}

fn latest_commit_age(pkg: &Package) -> Result<Option<(&str, u64)>> {
    let Some(url) = &pkg.repository else {
        return Ok(None);
    };

    let Some((url, timestamp)) = timestamp(url)? else {
        return Ok(None);
    };

    let duration = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;

    Ok(Some((url, duration.as_secs() - timestamp)))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn timestamp(url: &str) -> Result<Option<(&str, u64)>> {
    TIMESTAMP_CACHE.with_borrow_mut(|timestamp_cache| {
        let timestamp_cache = timestamp_cache.get_or_insert_with(HashMap::new);
        // smoelius: Check both the regular and the shortened url.
        if let Some(timestamp) = timestamp_cache.get(url) {
            return Ok(Some((url, *timestamp)));
        }
        if let Some(url) = shorten_url(url) {
            if let Some(timestamp) = timestamp_cache.get(url) {
                return Ok(Some((url, *timestamp)));
            }
        }
        let Some((url, timestamp)) = timestamp_from_clone(url)? else {
            return Ok(None);
        };
        timestamp_cache.insert(url.to_owned(), timestamp);
        Ok(Some((url, timestamp)))
    })
}

fn timestamp_from_clone(url: &str) -> Result<Option<(&str, u64)>> {
    let tempdir = tempdir().with_context(|| "failed to create temporary directory")?;

    let Some(url) = clone_repository(url, tempdir.path())? else {
        warn!("failed to clone `{}`", url);
        return Ok(None);
    };

    let mut command = Command::new("git");
    command
        .args(["log", "-1", "--pretty=format:%ct"])
        .current_dir(&tempdir);
    let output = command
        .output()
        .with_context(|| format!("failed to run command: {command:?}"))?;
    ensure!(output.status.success(), "command failed: {command:?}");

    let stdout = std::str::from_utf8(&output.stdout)?;
    let timestamp = u64::from_str(stdout.trim_end())?;

    Ok(Some((url, timestamp)))
}

fn clone_repository<'a>(url: &'a str, path: &Path) -> Result<Option<&'a str>> {
    let mut urls = vec![url];
    if let Some(url) = shorten_url(url) {
        urls.push(url);
    }
    let successful_url =
        urls.into_iter()
            .try_fold(None, |successful_url, url| -> Result<Option<&str>> {
                if successful_url.is_some() {
                    return Ok(successful_url);
                }
                let mut command = Command::new("git");
                command
                    .args([
                        "clone",
                        "--depth=1",
                        "--quiet",
                        url,
                        &path.to_string_lossy(),
                    ])
                    .stderr(Stdio::null());
                let status = command
                    .status()
                    .with_context(|| format!("failed to run command: {command:?}"))?;
                if status.success() {
                    Ok(Some(url))
                } else {
                    Ok(None)
                }
            })?;
    Ok(successful_url)
}

#[allow(clippy::unwrap_used)]
static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https://[^/]*/[^/]*/[^/]*").unwrap());

#[allow(clippy::unwrap_used)]
fn shorten_url(url: &str) -> Option<&str> {
    RE.captures(url)
        .map(|captures| captures.get(0).unwrap().as_str())
}

fn display_unmaintained_pkg(unmaintained_pkg: &UnmaintainedPkg) -> Result<()> {
    let UnmaintainedPkg {
        pkg,
        url_and_age,
        outdated_deps,
    } = unmaintained_pkg;
    let url_and_age_msg = if let Some((url, age)) = url_and_age {
        format!("{url} updated {} days ago", age / SECS_PER_DAY)
    } else {
        String::from("no repository")
    };
    println!("{} ({})", pkg.name, url_and_age_msg);
    for OutdatedDep {
        dep,
        version_used,
        version_latest,
    } in outdated_deps
    {
        println!(
            "    {} (requirement: {}, version used: {}, latest: {})",
            dep.name, dep.req, version_used, version_latest
        );
    }
    if opts::get().tree {
        display_path(&pkg.name, &pkg.version)?;
        println!();
    }
    Ok(())
}

fn display_path(name: &str, version: &Version) -> Result<()> {
    let spec = format!("{name}@{version}");
    let mut command = Command::new("cargo");
    command.args(["tree", "--workspace", "--invert", &spec]);
    let status = command
        .status()
        .with_context(|| format!("failed to run command: {command:?}"))?;
    ensure!(status.success(), "command failed: {command:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_cmd::prelude::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Opts::command().debug_assert();
    }

    #[test]
    fn usage() {
        std::process::Command::cargo_bin("cargo-unmaintained")
            .unwrap()
            .args(["unmaintained", "--help"])
            .assert()
            .success()
            .stdout(predicates::str::contains("Usage: cargo unmaintained"));
    }

    #[test]
    fn version() {
        std::process::Command::cargo_bin("cargo-unmaintained")
            .unwrap()
            .args(["unmaintained", "--version"])
            .assert()
            .success()
            .stdout(format!(
                "cargo-unmaintained {}\n",
                env!("CARGO_PKG_VERSION")
            ));
    }
}
