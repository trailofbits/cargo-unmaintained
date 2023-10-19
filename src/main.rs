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
    env::args,
    path::Path,
    process::{Command, Stdio},
    str::FromStr,
    time::SystemTime,
};
use tempfile::tempdir;

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
#[clap(version = crate_version!())]
struct Opts {
    #[clap(long, help = "Show paths to dependencies")]
    tree: bool,

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
    age: Option<u64>,
    outdated_deps: Vec<OutdatedDep<'a>>,
}

struct OutdatedDep<'a> {
    dep: &'a Dependency,
    version_used: &'a Version,
    version_latest: Version,
}

thread_local! {
    static INDEX: Lazy<GitIndex> = Lazy::new(|| GitIndex::new_cargo_default().unwrap());
}

macro_rules! warn {
    ($opts: expr, $format:literal, $($arg:expr),*) => {
        if !$opts.quiet {
            eprintln!(concat!("warning: ", $format), $($arg),*);
        }
    };
}

fn main() -> Result<()> {
    env_logger::init();

    let Cargo {
        subcmd: CargoSubCommand::Unmaintained(opts),
    } = Cargo::parse_from(args());

    let metadata = MetadataCommand::new().exec()?;

    let mut unnmaintained_pkgs = Vec::new();

    for pkg in &metadata.packages {
        let upgradeable_deps = outdated_deps(&opts, &metadata, pkg)?;

        if upgradeable_deps.is_empty() {
            continue;
        }

        let age = latest_commit_age(&opts, pkg)?;

        if age.map_or(false, |age| age < opts.max_age * SECS_PER_DAY) {
            continue;
        }

        unnmaintained_pkgs.push(UnmaintainedPkg {
            pkg,
            age,
            outdated_deps: upgradeable_deps,
        });
    }

    unnmaintained_pkgs.sort_by_key(|unmaintained| unmaintained.age);

    for unmaintained_pkg in unnmaintained_pkgs {
        display_unmaintained_pkg(&opts, &unmaintained_pkg)?;
    }

    Ok(())
}

fn outdated_deps<'a>(
    _opts: &Opts,
    metadata: &'a Metadata,
    pkg: &'a Package,
) -> Result<Vec<OutdatedDep<'a>>> {
    if !published(pkg) {
        return Ok(Vec::new());
    }
    let mut deps = Vec::new();
    for dep in &pkg.dependencies {
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
        let Some(krate) = INDEX.with(|index| index.crate_(&dep.name)) else {
            debug!("failed to find `{}` in index", &dep.name);
            debug_assert!(!published(dep_pkg));
            continue;
        };
        let version_latest_index = krate
            .highest_normal_version()
            .ok_or_else(|| anyhow!("`{}` has no normal version", &dep.name))?;
        let version_latest = Version::from_str(version_latest_index.version())?;
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

fn published(pkg: &Package) -> bool {
    pkg.publish
        .as_ref()
        .map_or(true, |registries| !registries.is_empty())
}

fn latest_commit_age(opts: &Opts, pkg: &Package) -> Result<Option<u64>> {
    let Some(repository) = &pkg.repository else {
        return Ok(None);
    };
    let tempdir = tempdir().with_context(|| "failed to create temporary directory")?;

    let success = clone_repository(repository, tempdir.path())?;
    if !success {
        warn!(opts, "failed to clone `{}`", repository);
        return Ok(None);
    }

    let latest_commit_time = latest_commit_time(tempdir.path())?;

    let duration = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;

    Ok(Some(duration.as_secs() - latest_commit_time))
}

fn clone_repository(repository: &str, path: &Path) -> Result<bool> {
    let mut urls = vec![repository];
    if let Some(url) = shortened_url(repository) {
        urls.push(url);
    }
    let success = urls
        .into_iter()
        .try_fold(false, |success, url| -> Result<bool> {
            if success {
                return Ok(success);
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
            Ok(status.success())
        })?;
    Ok(success)
}

static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https://[^/]*/[^/]*/[^/]*").unwrap());

fn shortened_url(url: &str) -> Option<&str> {
    RE.captures(url)
        .map(|captures| captures.get(0).unwrap().as_str())
}

fn latest_commit_time(path: &Path) -> Result<u64> {
    let mut command = Command::new("git");
    command
        .args(["log", "-1", "--pretty=format:%ct"])
        .current_dir(path);
    let output = command
        .output()
        .with_context(|| format!("failed to run command: {command:?}"))?;
    ensure!(output.status.success(), "command failed: {command:?}");

    let stdout = std::str::from_utf8(&output.stdout)?;
    u64::from_str(stdout.trim_end()).map_err(Into::into)
}

fn display_unmaintained_pkg(opts: &Opts, unmaintained_pkg: &UnmaintainedPkg) -> Result<()> {
    let UnmaintainedPkg {
        pkg,
        age,
        outdated_deps,
    } = unmaintained_pkg;
    let repo_msg = pkg.repository.as_deref().unwrap_or("no repository");
    let age_msg = age
        .map(|age| format!(" last updated {} days ago", age / SECS_PER_DAY))
        .unwrap_or_default();
    println!("{} ({}{})", pkg.name, repo_msg, age_msg);
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
    if opts.tree {
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
