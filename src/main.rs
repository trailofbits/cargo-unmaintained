#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use anyhow::{anyhow, bail, ensure, Context, Result};
use cargo_metadata::{
    semver::{Version, VersionReq},
    Dependency, DependencyKind, Metadata, MetadataCommand, Package,
};
use clap::{crate_version, Parser};
use crates_index::GitIndex;
use home::cargo_home;
use log::debug;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    cell::RefCell,
    collections::HashMap,
    env::{args, var},
    ffi::OsStr,
    fs::{read_to_string, File},
    path::{Path, PathBuf},
    process::{exit, Command, Stdio},
    str::FromStr,
    time::SystemTime,
};
use tempfile::tempdir;
use toml::{Table, Value};
use walkdir::WalkDir;

mod github;
mod opts;
mod verbose;

#[cfg(all(unix, feature = "lock_index"))]
mod flock;

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

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Parser)]
#[remain::sorted]
#[clap(
    version = crate_version!(),
    about = "Find unmaintained dependencies in Rust projects",
    after_help = "\
The `GITHUB_TOKEN` environment variable can be set to the path of a file containing a personal \
access token, which will be used to authenticate to GitHub.

Unless --no-exit-code is passed, the exit status is 0 if no unmaintained dependencies were found \
and no irrecoverable errors occurred, 1 if unmaintained dependencies were found, and 2 if an \
irrecoverable error occurred."
)]
struct Opts {
    #[clap(
        long,
        help = "Exit as soon as an unmaintained dependency is found",
        conflicts_with = "no_exit_code"
    )]
    fail_fast: bool,

    #[clap(
        long,
        help = "Do not check whether package repository contains package; enables checking last \
                commit timestamps using the GitHub API, which is faster, but can produce false \
                negatives"
    )]
    imprecise: bool,

    #[clap(
        long,
        help = "Age in days that a repository's last commit must not exceed for the repository to \
                be considered current; 0 effectively disables this check, though ages are still \
                reported",
        value_name = "DAYS",
        default_value = "365"
    )]
    max_age: u64,

    #[clap(
        long,
        help = "Do not set exit status when unmaintained dependencies are found",
        conflicts_with = "fail_fast"
    )]
    no_exit_code: bool,

    #[clap(long, help = "Do not show warnings")]
    no_warnings: bool,

    #[clap(
        long,
        short,
        help = "Check only whether package SPEC is unmaintained",
        value_name = "SPEC"
    )]
    package: Option<String>,

    #[clap(long, help = "Show paths to unmaintained dependencies")]
    tree: bool,

    #[clap(long, help = "Show information about what cargo-unmaintained is doing")]
    verbose: bool,
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

struct DepReq<'a> {
    name: &'a str,
    req: VersionReq,
}

impl<'a> DepReq<'a> {
    fn new(name: &'a str, req: VersionReq) -> Self {
        Self { name, req }
    }
}

impl<'a> From<&'a Dependency> for DepReq<'a> {
    fn from(value: &'a Dependency) -> Self {
        Self {
            name: &value.name,
            req: value.req.clone(),
        }
    }
}

thread_local! {
    #[allow(clippy::unwrap_used)]
    static INDEX: Lazy<GitIndex> = Lazy::new(|| {
        let _lock = lock_index().unwrap();
        GitIndex::new_cargo_default().unwrap()
    });
    static LATEST_VERSION_CACHE: RefCell<Option<HashMap<String, Version>>> = RefCell::new(None);
    static TIMESTAMP_CACHE: RefCell<Option<HashMap<String, Option<u64>>>> = RefCell::new(None);
    static REPOSITORY_CACHE: RefCell<Option<HashMap<String, Option<PathBuf>>>> = RefCell::new(None);
}

macro_rules! warn {
    ($fmt:expr, $($arg:tt)*) => {
        if crate::opts::get().no_warnings {
            debug!($fmt, $($arg)*);
        } else {
            verbose::newline!();
            eprintln!(concat!("warning: ", $fmt), $($arg)*);
        }
    };
}

fn main() -> Result<()> {
    env_logger::init();

    if let Ok(path) = var("GITHUB_TOKEN") {
        github::load_token(&path)?;
    }

    let Cargo {
        subcmd: CargoSubCommand::Unmaintained(opts),
    } = Cargo::parse_from(args());

    opts::init(opts);

    match unmaintained() {
        Ok(false) => exit(0),
        Ok(true) => exit(1),
        Err(error) => {
            eprintln!("Error: {error}");
            exit(2);
        }
    }
}

fn unmaintained() -> Result<bool> {
    let mut unnmaintained_pkgs = Vec::new();

    let metadata = MetadataCommand::new().exec()?;

    let packages = maybe_filter_packages(&metadata)?;

    for pkg in packages {
        let outdated_deps = outdated_deps(&metadata, pkg)?;

        if outdated_deps.is_empty() {
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
            outdated_deps,
        });

        if opts::get().fail_fast {
            break;
        }
    }

    unnmaintained_pkgs
        .sort_by_key(|unmaintained| unmaintained.url_and_age.as_ref().map(|&(_, age)| age));

    for unmaintained_pkg in &unnmaintained_pkgs {
        display_unmaintained_pkg(unmaintained_pkg)?;
    }

    Ok(!opts::get().no_exit_code && !unnmaintained_pkgs.is_empty())
}

fn maybe_filter_packages(metadata: &Metadata) -> Result<Vec<&Package>> {
    let Some(spec) = &opts::get().package else {
        return Ok(metadata.packages.iter().collect());
    };

    let dep_req = if let Some((name, req)) = spec.split_once('@') {
        let req = VersionReq::from_str(req)?;
        DepReq::new(name, req)
    } else {
        DepReq::new(spec, VersionReq::STAR)
    };

    let packages = find_packages(metadata, dep_req).collect::<Vec<_>>();

    if packages.len() >= 2 {
        bail!("found multiple packages matching `{spec}`: {:#?}", packages);
    }

    if packages.is_empty() {
        bail!("found no packages matching `{spec}`");
    }

    Ok(packages)
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
        let Some(dep_pkg) = find_packages(metadata, dep.into()).next() else {
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
    // smoelius: A dependency could appear more than once, e.g., because it is used with different
    // features as a normal and as a development dependency.
    deps.dedup_by(|lhs, rhs| lhs.dep.name == rhs.dep.name && lhs.dep.req == rhs.dep.req);
    Ok(deps)
}

fn find_packages<'a>(
    metadata: &'a Metadata,
    dep_req: DepReq<'a>,
) -> impl Iterator<Item = &'a Package> {
    metadata
        .packages
        .iter()
        .filter(move |pkg| dep_req.name == pkg.name && dep_req.req.matches(&pkg.version))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn latest_version(name: &str) -> Result<Version> {
    LATEST_VERSION_CACHE.with_borrow_mut(|latest_version_cache| {
        let latest_version_cache = latest_version_cache.get_or_insert_with(HashMap::new);
        if let Some(version) = latest_version_cache.get(name) {
            return Ok(version.clone());
        }
        verbose::wrap!(
            || {
                let krate = INDEX.with(|index| {
                    let _ = Lazy::force(index);
                    let _lock = lock_index()?;
                    index
                        .crate_(name)
                        .ok_or_else(|| anyhow!("failed to find `{}` in index", name))
                })?;
                let latest_version_index = krate
                    .highest_normal_version()
                    .ok_or_else(|| anyhow!("`{}` has no normal version", name))?;
                let latest_version = Version::from_str(latest_version_index.version())?;
                latest_version_cache.insert(name.to_owned(), latest_version.clone());
                Ok(latest_version)
            },
            "latest version of `{}` using crates.io index",
            name,
        )
    })
}

fn published(pkg: &Package) -> bool {
    pkg.publish
        .as_ref()
        .map_or(true, |registries| !registries.is_empty())
}

fn latest_commit_age(pkg: &Package) -> Result<Option<(&str, u64)>> {
    let Some((url, timestamp)) = timestamp(pkg)? else {
        return Ok(None);
    };

    let duration = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;

    Ok(Some((url, duration.as_secs() - timestamp)))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn timestamp(pkg: &Package) -> Result<Option<(&str, u64)>> {
    TIMESTAMP_CACHE.with_borrow_mut(|timestamp_cache| {
        let timestamp_cache = timestamp_cache.get_or_insert_with(HashMap::new);
        // smoelius: Check both the regular and the shortened url.
        for url_cached in urls(pkg) {
            if let Some(timestamp) = timestamp_cache.get(url_cached) {
                if opts::get().imprecise {
                    return Ok(timestamp.map(|timestamp| (url_cached, timestamp)));
                }
                // smoelius: If the timestamp is `None` then don't bother checking the repository
                // cache. It could be that a previous attempt to clone the repository failed because
                // of spurious network errors, for example.
                if timestamp.is_none() {
                    return Ok(None);
                }
                // smoelius: `pkg`'s repository could contain other packages that were already
                // timestamped. Thus, `pkg`'s repository could already be in the timestamp cache.
                // But in that case, we still need to verify that `pkg` appears in its repository.
                #[allow(clippy::panic)]
                let Some((url_cloned, repo_dir)) = clone_repository(pkg)?
                else {
                    panic!("url in timestamp cache is uncloneable: {url_cached}");
                };
                assert_eq!(url_cached, url_cloned);
                if verify_membership(pkg, &repo_dir)? {
                    return Ok(timestamp.map(|timestamp| (url_cached, timestamp)));
                }
            }
        }
        verbose::wrap!(
            || {
                if let Some((url, timestamp)) = timestamp_uncached(pkg)? {
                    timestamp_cache.insert(url.to_owned(), Some(timestamp));
                    return Ok(Some((url, timestamp)));
                }
                // smoelius: In the event of failure, set all urls associated with the repository to
                // `None`.
                for url in urls(pkg) {
                    timestamp_cache.insert(url.to_owned(), None);
                }
                Ok(None)
            },
            "timestamp of `{}`",
            pkg.name
        )
    })
}

fn timestamp_uncached(pkg: &Package) -> Result<Option<(&str, u64)>> {
    let Some(url) = &pkg.repository else {
        return Ok(None);
    };

    if opts::get().imprecise && url.starts_with("https://github.com/") {
        verbose::update!("using GitHub API");

        match github::timestamp(url) {
            Ok((url, timestamp)) => {
                return Ok(Some((url, timestamp)));
            }
            Err(error) => {
                if var("GITHUB_TOKEN").is_err() {
                    debug!(
                        "failed to get timestamp for {} using GitHub API: {}",
                        url, error
                    );
                } else {
                    warn!(
                        "failed to get timestamp for {} using GitHub API: {}",
                        url,
                        first_line(&error.to_string())
                    );
                }
                verbose::update!("falling back to shallow clone");
            }
        }
    } else {
        verbose::update!("using shallow clone");
    }

    timestamp_from_clone(pkg)
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

fn timestamp_from_clone(pkg: &Package) -> Result<Option<(&str, u64)>> {
    let Some((url, repo_dir)) = clone_repository(pkg)? else {
        return Ok(None);
    };

    if !opts::get().imprecise && !verify_membership(pkg, &repo_dir)? {
        return Ok(None);
    }

    let mut command = Command::new("git");
    command
        .args(["log", "-1", "--pretty=format:%ct"])
        .current_dir(repo_dir);
    let output = command
        .output()
        .with_context(|| format!("failed to run command: {command:?}"))?;
    ensure!(output.status.success(), "command failed: {command:?}");

    let stdout = std::str::from_utf8(&output.stdout)?;
    let timestamp = u64::from_str(stdout.trim_end())?;

    Ok(Some((url, timestamp)))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn clone_repository(pkg: &Package) -> Result<Option<(&str, PathBuf)>> {
    REPOSITORY_CACHE.with_borrow_mut(|repository_cache| {
        let repository_cache = repository_cache.get_or_insert_with(HashMap::new);
        // smoelius: Check all urls associated with the package.
        for url in urls(pkg) {
            if let Some(repo_dir) = repository_cache.get(url) {
                return Ok(repo_dir.clone().map(|repo_dir| (url, repo_dir)));
            }
        }
        if let Some((url, repo_dir)) = clone_repository_uncached(pkg)? {
            repository_cache.insert(url.to_owned(), Some(repo_dir.clone()));
            return Ok(Some((url, repo_dir)));
        }
        if let Some(url) = &pkg.repository {
            warn!("failed to clone `{}`", url);
        }
        // smoelius: In the event of failure, set all urls associated with the repository to `None`.
        for url in urls(pkg) {
            repository_cache.insert(url.to_owned(), None);
        }
        Ok(None)
    })
}

fn clone_repository_uncached(pkg: &Package) -> Result<Option<(&str, PathBuf)>> {
    urls(pkg).into_iter().try_fold(
        None,
        |successful_url_and_path, url| -> Result<Option<(&str, PathBuf)>> {
            if successful_url_and_path.is_some() {
                return Ok(successful_url_and_path);
            }
            let tempdir = tempdir().with_context(|| "failed to create temporary directory")?;
            let mut command = Command::new("git");
            command
                .args([
                    "clone",
                    "--depth=1",
                    "--quiet",
                    url,
                    &tempdir.path().to_string_lossy(),
                ])
                .env_remove("GIT_ASKPASS")
                .env("GIT_TERMINAL_PROMPT", "0")
                .stderr(Stdio::null());
            let status = command
                .status()
                .with_context(|| format!("failed to run command: {command:?}"))?;
            if status.success() {
                // smoelius: Leak temporary directory.
                Ok(Some((url, tempdir.into_path())))
            } else {
                Ok(None)
            }
        },
    )
}

fn urls(pkg: &Package) -> impl IntoIterator<Item = &str> {
    let mut urls = Vec::new();

    if let Some(url) = &pkg.repository {
        // smoelius: Without the use of `trim_trailing_slash`, whether a timestamp was obtained via
        // the GitHub API or a shallow clone would be distinguishable.
        let url = trim_trailing_slash(url);

        urls.push(url);

        if let Some(shortened_url) = shorten_url(url) {
            urls.push(shortened_url);
        }
    }

    urls
}

fn trim_trailing_slash(url: &str) -> &str {
    url.strip_suffix('/').unwrap_or(url)
}

#[allow(clippy::unwrap_used)]
static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https://[^/]*/[^/]*/[^/]*").unwrap());

#[allow(clippy::unwrap_used)]
fn shorten_url(url: &str) -> Option<&str> {
    RE.captures(url)
        .map(|captures| captures.get(0).unwrap().as_str())
}

fn verify_membership(pkg: &Package, repo_dir: &Path) -> Result<bool> {
    for entry in WalkDir::new(repo_dir) {
        let entry = entry?;
        let path = entry.path();
        if path.file_name() != Some(OsStr::new("Cargo.toml")) {
            continue;
        }
        let contents = read_to_string(path).with_context(|| format!("failed to read {path:?}"))?;
        let Ok(table) = contents.parse::<Table>().map_err(|error| {
            warn!(
                "failed to parse {:?}: {}",
                path,
                error.to_string().trim_end()
            );
        }) else {
            continue;
        };
        if table
            .get("package")
            .and_then(Value::as_table)
            .and_then(|table| table.get("name"))
            .and_then(Value::as_str)
            == Some(&pkg.name)
        {
            return Ok(true);
        }
    }

    Ok(false)
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

static INDEX_PATH: Lazy<PathBuf> = Lazy::new(|| {
    #[allow(clippy::unwrap_used)]
    let cargo_home = cargo_home().unwrap();
    cargo_home.join("registry/index")
});

#[cfg(all(unix, feature = "lock_index"))]
fn lock_index() -> Result<File> {
    flock::lock_path(&INDEX_PATH).with_context(|| format!("failed to lock {INDEX_PATH:?}"))
}

#[cfg(not(all(unix, feature = "lock_index")))]
fn lock_index() -> Result<File> {
    File::open(&*INDEX_PATH).with_context(|| format!("failed to open {INDEX_PATH:?}"))
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
