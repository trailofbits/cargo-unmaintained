#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#![cfg_attr(dylint_lib = "general", allow(crate_wide_allow))]
#![cfg_attr(dylint_lib = "supplementary", allow(nonexistent_path_in_comment))]

use anyhow::{Context, Result, anyhow, bail, ensure};
use cargo_metadata::{
    Dependency, DependencyKind, Metadata, MetadataCommand, Package,
    semver::{Version, VersionReq},
};
use clap::{Parser, crate_version};
use crates_index::GitIndex;
use home::cargo_home;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env::args,
    ffi::OsStr,
    fs::File,
    io::{BufRead, IsTerminal},
    path::{Path, PathBuf},
    process::{Command, Stdio, exit},
    str::FromStr,
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};
use tempfile::TempDir;
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};
use toml::{Table, Value};

pub mod flush;
pub mod github;
pub mod packaging;

mod curl;
mod on_disk_cache;
mod opts;
mod progress;
mod serialize;
mod verbose;

#[cfg(feature = "lock-index")]
mod flock;

use github::{Github as _, Impl as Github};

mod repo_status;
use repo_status::RepoStatus;

mod url;
use url::{Url, urls};

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

include!(concat!(env!("OUT_DIR"), "/after_help.rs"));

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Parser)]
#[remain::sorted]
#[clap(
    version = crate_version!(),
    about = "Find unmaintained packages in Rust projects",
    after_help = AFTER_HELP
)]
struct Opts {
    #[clap(
        long,
        help = "When to use color: always, auto, or never",
        default_value = "auto",
        value_name = "WHEN"
    )]
    color: ColorChoice,

    #[clap(
        long,
        help = "Exit as soon as an unmaintained package is found",
        conflicts_with = "no_exit_code"
    )]
    fail_fast: bool,

    #[clap(long, help = "Output JSON (experimental)")]
    json: bool,

    #[clap(
        long,
        help = "Age in days that a repository's last commit must not exceed for the repository to \
                be considered current; 0 effectively disables this check, though ages are still \
                reported",
        value_name = "DAYS",
        default_value = "365"
    )]
    max_age: u64,

    #[cfg(all(feature = "on-disk-cache", not(windows)))]
    #[clap(long, help = "Do not cache data on disk for future runs")]
    no_cache: bool,

    #[clap(
        long,
        help = "Do not set exit status when unmaintained packages are found",
        conflicts_with = "fail_fast"
    )]
    no_exit_code: bool,

    #[clap(long, help = "Do not show warnings")]
    no_warnings: bool,

    #[clap(
        long,
        short,
        help = "Check only whether package NAME is unmaintained",
        value_name = "NAME"
    )]
    package: Option<String>,

    #[cfg(all(feature = "on-disk-cache", not(windows)))]
    #[clap(long, help = "Remove all cached data from disk and exit")]
    purge: bool,

    #[cfg(not(windows))]
    #[clap(
        long,
        help = "Read a personal access token from standard input and save it to \
                $HOME/.config/cargo-unmaintained/token.txt"
    )]
    save_token: bool,

    #[cfg(windows)]
    #[clap(
        long,
        help = "Read a personal access token from standard input and save it to \
                %LOCALAPPDATA%\\cargo-unmaintained\\token.txt"
    )]
    save_token: bool,

    #[clap(long, help = "Show paths to unmaintained packages")]
    tree: bool,

    #[clap(long, help = "Show information about what cargo-unmaintained is doing")]
    verbose: bool,
}

struct UnmaintainedPkg<'a> {
    pkg: &'a Package,
    repo_age: RepoStatus<'a, u64>,
    newer_version_is_available: bool,
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
    #[allow(dead_code)]
    fn new(name: &'a str, req: VersionReq) -> Self {
        Self { name, req }
    }

    fn matches(&self, pkg: &Package) -> bool {
        self.name == pkg.name && self.req.matches(&pkg.version)
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

#[macro_export]
macro_rules! warn {
    ($fmt:expr, $($arg:tt)*) => {
        if $crate::opts::get().no_warnings {
            log::debug!($fmt, $($arg)*);
        } else {
            $crate::verbose::newline!();
            $crate::PROGRESS.with_borrow_mut(|progress| progress.as_mut().map($crate::progress::Progress::newline));
            eprintln!(concat!("warning: ", $fmt), $($arg)*);
        }
    };
}

thread_local! {
    #[allow(clippy::unwrap_used)]
    static INDEX: LazyLock<GitIndex> = LazyLock::new(|| {
        let _lock = lock_index().unwrap();
        let mut index = GitIndex::new_cargo_default().unwrap();
        if let Err(error) = index.update() {
            warn!("failed to update index: {}", error);
        }
        index
    });
    static PROGRESS: RefCell<Option<progress::Progress>> = const { RefCell::new(None) };
    // smoelius: The next four statics are "in-memory" caches.
    // smoelius: Note that repositories are (currently) stored in both an in-memory cache and an
    // on-disk cache. The former is keyed by url; the latter is keyed by package.
    // smoelius: A reason for having the former is the following. Multiple packages map to the same
    // url, and multiple urls map to the same shortened url. Thus, a cache keyed by url has a
    // greater chance of a cache hit.
    static GENERAL_STATUS_CACHE: RefCell<HashMap<Url<'static>, RepoStatus<'static, ()>>> = RefCell::new(HashMap::new());
    static LATEST_VERSION_CACHE: RefCell<HashMap<String, Version>> = RefCell::new(HashMap::new());
    static TIMESTAMP_CACHE: RefCell<HashMap<Url<'static>, RepoStatus<'static, SystemTime>>> = RefCell::new(HashMap::new());
    static REPOSITORY_CACHE: RefCell<HashMap<Url<'static>, RepoStatus<'static, PathBuf>>> = RefCell::new(HashMap::new());
}

static TOKEN_FOUND: AtomicBool = AtomicBool::new(false);

pub fn run() -> Result<()> {
    env_logger::init();

    let Cargo {
        subcmd: CargoSubCommand::Unmaintained(opts),
    } = Cargo::parse_from(args());

    opts::init(opts);

    if opts::get().save_token {
        // smoelius: Currently, if additional options are passed besides --save-token, they are
        // ignored and no error is emitted. This is ugly.
        return Github::save_token();
    }

    #[cfg(all(feature = "on-disk-cache", not(windows)))]
    if opts::get().purge {
        return on_disk_cache::purge_cache();
    }

    if Github::load_token(|_| Ok(()))? {
        TOKEN_FOUND.store(true, Ordering::SeqCst);
    }

    match unmaintained() {
        Ok(false) => exit(0),
        Ok(true) => exit(1),
        Err(error) => {
            eprintln!("Error: {error:?}");
            exit(2);
        }
    }
}

fn unmaintained() -> Result<bool> {
    let mut unmaintained_pkgs = Vec::new();

    let metadata = metadata()?;

    let packages = packages(&metadata)?;

    eprintln!(
        "Scanning {} packages and their dependencies{}",
        packages.len(),
        if opts::get().verbose {
            ""
        } else {
            " (pass --verbose for more information)"
        }
    );

    if std::io::stderr().is_terminal() && !opts::get().verbose {
        PROGRESS
            .with_borrow_mut(|progress| *progress = Some(progress::Progress::new(packages.len())));
    }

    for pkg in packages {
        PROGRESS.with_borrow_mut(|progress| {
            progress
                .as_mut()
                .map_or(Ok(()), |progress| progress.advance(&pkg.name))
        })?;

        if let Some(mut unmaintained_pkg) = is_unmaintained_package(&metadata, pkg)? {
            // smoelius: Before considering a package unmaintained, verify that its latest version
            // would be considered unmaintained as well. Note that we still report the details of
            // the version currently used. We may want to revisit this in the future.
            let newer_version_is_available = newer_version_is_available(pkg)?;
            if !newer_version_is_available || latest_version_is_unmaintained(&pkg.name)? {
                unmaintained_pkg.newer_version_is_available = newer_version_is_available;
                unmaintained_pkgs.push(unmaintained_pkg);

                if opts::get().fail_fast {
                    break;
                }
            }
        }
    }

    PROGRESS
        .with_borrow_mut(|progress| progress.as_mut().map_or(Ok(()), progress::Progress::finish))?;

    if opts::get().json {
        unmaintained_pkgs.sort_by_key(|unmaintained| &unmaintained.pkg.id);

        let json = serde_json::to_string_pretty(&unmaintained_pkgs)?;

        println!("{json}");
    } else {
        if unmaintained_pkgs.is_empty() {
            eprintln!("No unmaintained packages found");
            return Ok(false);
        }

        unmaintained_pkgs.sort_by_key(|unmaintained| unmaintained.repo_age.erase_url());

        display_unmaintained_pkgs(&unmaintained_pkgs)?;
    }

    Ok(!opts::get().no_exit_code)
}

fn metadata() -> Result<Metadata> {
    let mut command = MetadataCommand::new();

    // smoelius: See tests/snapbox.rs for another use of this conditional initialization trick.
    let tempdir: TempDir;

    if let Some(name) = &opts::get().package {
        tempdir = packaging::temp_package(name)?;
        command.current_dir(tempdir.path());
    }

    command.exec().map_err(Into::into)
}

fn packages(metadata: &Metadata) -> Result<Vec<&Package>> {
    let ignored_packages = ignored_packages(metadata)?;

    for name in &ignored_packages {
        if !metadata.packages.iter().any(|pkg| pkg.name == *name) {
            warn!(
                "workspace metadata says to ignore `{}`, but workspace does not depend upon `{}`",
                name, name
            );
        }
    }

    filter_packages(metadata, &ignored_packages)
}

#[derive(serde::Deserialize)]
struct UnmaintainedMetadata {
    ignore: Option<Vec<String>>,
}

fn ignored_packages(metadata: &Metadata) -> Result<HashSet<String>> {
    let serde_json::Value::Object(object) = &metadata.workspace_metadata else {
        return Ok(HashSet::default());
    };
    let Some(value) = object.get("unmaintained") else {
        return Ok(HashSet::default());
    };
    let metadata = serde_json::value::from_value::<UnmaintainedMetadata>(value.clone())?;
    Ok(metadata.ignore.unwrap_or_default().into_iter().collect())
}

fn filter_packages<'a>(
    metadata: &'a Metadata,
    ignored_packages: &HashSet<String>,
) -> Result<Vec<&'a Package>> {
    let mut packages = Vec::new();

    // smoelius: If a project relies on multiple versions of a package, check only the latest one.
    let metadata_latest_version_map = build_metadata_latest_version_map(metadata);

    for pkg in &metadata.packages {
        // smoelius: Don't consider whether workspace members are unmaintained.
        if metadata.workspace_members.contains(&pkg.id) {
            continue;
        }

        if ignored_packages.contains(&pkg.name) {
            continue;
        }

        #[allow(clippy::panic)]
        let version = metadata_latest_version_map
            .get(&pkg.name)
            .unwrap_or_else(|| {
                panic!(
                    "`metadata_latest_version_map` does not contain {}",
                    pkg.name
                )
            });

        if pkg.version != *version {
            continue;
        }

        if let Some(name) = &opts::get().package {
            if pkg.name != *name {
                continue;
            }
        }

        packages.push(pkg);
    }

    if let Some(name) = &opts::get().package {
        if packages.len() >= 2 {
            bail!("found multiple packages matching `{name}`: {:#?}", packages);
        }

        if packages.is_empty() {
            bail!("found no packages matching `{name}`");
        }
    }

    Ok(packages)
}

fn build_metadata_latest_version_map(metadata: &Metadata) -> HashMap<String, Version> {
    let mut map: HashMap<String, Version> = HashMap::new();

    for pkg in &metadata.packages {
        if let Some(version) = map.get_mut(&pkg.name) {
            if *version < pkg.version {
                *version = pkg.version.clone();
            }
        } else {
            map.insert(pkg.name.clone(), pkg.version.clone());
        }
    }

    map
}

fn newer_version_is_available(pkg: &Package) -> Result<bool> {
    if pkg
        .source
        .as_ref()
        .is_none_or(|source| !source.is_crates_io())
    {
        return Ok(false);
    }

    let latest_version = latest_version(&pkg.name)?;

    Ok(pkg.version != latest_version)
}

fn latest_version_is_unmaintained(name: &str) -> Result<bool> {
    let tempdir = packaging::temp_package(name)?;

    let metadata = MetadataCommand::new().current_dir(tempdir.path()).exec()?;

    #[allow(clippy::panic)]
    let pkg = metadata
        .packages
        .iter()
        .find(|pkg| name == pkg.name)
        .unwrap_or_else(|| panic!("failed to find package `{name}`"));

    let unmaintained_package = is_unmaintained_package(&metadata, pkg)?;

    Ok(unmaintained_package.is_some())
}

fn is_unmaintained_package<'a>(
    metadata: &'a Metadata,
    pkg: &'a Package,
) -> Result<Option<UnmaintainedPkg<'a>>> {
    if let Some(url_string) = &pkg.repository {
        let can_use_github_api =
            TOKEN_FOUND.load(Ordering::SeqCst) && url_string.starts_with("https://github.com/");

        let url = url_string.as_str().into();

        if can_use_github_api {
            let repo_status = general_status(&pkg.name, url)?;
            if repo_status.is_failure() {
                return Ok(Some(UnmaintainedPkg {
                    pkg,
                    repo_age: repo_status.map_failure(),
                    newer_version_is_available: false,
                    outdated_deps: Vec::new(),
                }));
            }
        }

        let repo_status = clone_repository(pkg)?;
        if repo_status.is_failure() {
            // smoelius: Mercurial repos get a pass.
            if matches!(repo_status, RepoStatus::Uncloneable(_)) && curl::is_mercurial_repo(url)? {
                return Ok(None);
            }
            return Ok(Some(UnmaintainedPkg {
                pkg,
                repo_age: repo_status.map_failure(),
                newer_version_is_available: false,
                outdated_deps: Vec::new(),
            }));
        }
    }

    let outdated_deps = outdated_deps(metadata, pkg)?;

    if outdated_deps.is_empty() {
        return Ok(None);
    }

    let repo_age = latest_commit_age(pkg)?;

    if repo_age
        .as_success()
        .is_some_and(|(_, &age)| age < opts::get().max_age * SECS_PER_DAY)
    {
        return Ok(None);
    }

    Ok(Some(UnmaintainedPkg {
        pkg,
        repo_age,
        newer_version_is_available: false,
        outdated_deps,
    }))
}

fn general_status(name: &str, url: Url) -> Result<RepoStatus<'static, ()>> {
    GENERAL_STATUS_CACHE.with_borrow_mut(|general_status_cache| {
        if let Some(&value) = general_status_cache.get(&url) {
            return Ok(value);
        }
        let to_string: &dyn Fn(&RepoStatus<'static, ()>) -> String;
        let (use_github_api, what, how) = if TOKEN_FOUND.load(Ordering::SeqCst)
            && url.as_str().starts_with("https://github.com/")
        {
            to_string = &RepoStatus::to_archival_status_string;
            (true, "archival status", "GitHub API")
        } else {
            to_string = &RepoStatus::to_existence_string;
            (false, "existence", "HTTP request")
        };
        verbose::wrap!(
            || {
                let repo_status = if use_github_api {
                    Github::archival_status(url)
                } else {
                    curl::existence(url)
                }
                .unwrap_or_else(|error| {
                    warn!("failed to determine `{}` {}: {}", name, what, error);
                    RepoStatus::Success(url, ())
                })
                .leak_url();
                general_status_cache.insert(url.leak(), repo_status);
                Ok(repo_status)
            },
            to_string,
            "{} of `{}` using {}",
            what,
            name,
            how
        )
    })
}

#[allow(clippy::unnecessary_wraps)]
fn outdated_deps<'a>(metadata: &'a Metadata, pkg: &'a Package) -> Result<Vec<OutdatedDep<'a>>> {
    if !published(pkg) {
        return Ok(Vec::new());
    }
    let mut deps = Vec::new();
    for dep in &pkg.dependencies {
        // smoelius: Don't check dependencies in private registries.
        if dep.registry.is_some() {
            continue;
        }
        // smoelius: Don't check dependencies specified by path.
        if dep.path.is_some() {
            continue;
        }
        let Some(dep_pkg) = find_packages(metadata, dep.into()).next() else {
            debug_assert!(dep.kind == DependencyKind::Development || dep.optional);
            continue;
        };
        let Ok(version_latest) = latest_version(&dep.name).map_err(|error| {
            // smoelius: I don't understand why a package can fail to be in the index, but I have
            // seen it happen.
            warn!("failed to get latest version of `{}`: {}", dep.name, error);
        }) else {
            continue;
        };
        if dep_pkg.version <= version_latest && !dep.req.matches(&version_latest) {
            let versions = versions(&dep_pkg.name)?;
            // smoelius: Require at least one incompatible version of the dependency that is more
            // than `max_age` days old.
            if versions
                .iter()
                .try_fold(false, |init, version| -> Result<_> {
                    if init {
                        return Ok(true);
                    }
                    let duration = SystemTime::now().duration_since(version.created_at.into())?;
                    let version_num = Version::parse(&version.num)?;
                    Ok(duration.as_secs() >= opts::get().max_age * SECS_PER_DAY
                        && dep_pkg.version <= version_num
                        && !dep.req.matches(&version_num))
                })?
            {
                deps.push(OutdatedDep {
                    dep,
                    version_used: &dep_pkg.version,
                    version_latest,
                });
            }
        }
    }
    // smoelius: A dependency could appear more than once, e.g., because it is used with different
    // features as a normal and as a development dependency.
    deps.dedup_by(|lhs, rhs| lhs.dep.name == rhs.dep.name && lhs.dep.req == rhs.dep.req);
    Ok(deps)
}

fn published(pkg: &Package) -> bool {
    pkg.publish.as_deref() != Some(&[])
}

fn find_packages<'a>(
    metadata: &'a Metadata,
    dep_req: DepReq<'a>,
) -> impl Iterator<Item = &'a Package> {
    metadata
        .packages
        .iter()
        .filter(move |pkg| dep_req.matches(pkg))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn latest_version(name: &str) -> Result<Version> {
    LATEST_VERSION_CACHE.with_borrow_mut(|latest_version_cache| {
        if let Some(version) = latest_version_cache.get(name) {
            return Ok(version.clone());
        }
        verbose::wrap!(
            || {
                let krate = INDEX.with(|index| {
                    let _ = LazyLock::force(index);
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
            ToString::to_string,
            "latest version of `{}` using crates.io index",
            name,
        )
    })
}

fn versions(name: &str) -> Result<Vec<crates_io_api::Version>> {
    on_disk_cache::with_cache(|cache| -> Result<_> {
        verbose::wrap!(
            || { cache.fetch_versions(name) },
            |versions: &[crates_io_api::Version]| format!("{} versions", versions.len()),
            "versions of `{}` using crates.io API",
            name
        )
    })
}

fn latest_commit_age(pkg: &Package) -> Result<RepoStatus<'_, u64>> {
    let repo_status = timestamp(pkg)?;

    repo_status
        .map(|timestamp| {
            let duration = SystemTime::now().duration_since(timestamp)?;

            Ok(duration.as_secs())
        })
        .transpose()
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn timestamp(pkg: &Package) -> Result<RepoStatus<'_, SystemTime>> {
    TIMESTAMP_CACHE.with_borrow_mut(|timestamp_cache| {
        // smoelius: Check both the regular and the shortened url.
        for url in urls(pkg) {
            if let Some(&repo_status) = timestamp_cache.get(&url) {
                // smoelius: If a previous attempt to timestamp the repository failed (e.g., because
                // of spurious network errors), then don't bother checking the repository cache.
                let Some((url_timestamped, &timestamp)) = repo_status.as_success() else {
                    return Ok(repo_status);
                };
                assert_eq!(url, url_timestamped);
                // smoelius: `pkg`'s repository could contain other packages that were already
                // timestamped. Thus, `pkg`'s repository could already be in the timestamp cache.
                // But in that case, we still need to verify that `pkg` appears in its repository.
                let repo_status = clone_repository(pkg)?;
                let Some((url_cloned, _)) = repo_status.as_success() else {
                    return Ok(repo_status.map_failure());
                };
                assert_eq!(url, url_cloned);
                return Ok(RepoStatus::Success(url, timestamp));
            }
        }
        let repo_status = timestamp_uncached(pkg)?;
        if let Some((url, _)) = repo_status.as_success() {
            timestamp_cache.insert(url.leak(), repo_status.leak_url());
        } else {
            // smoelius: In the event of failure, set all urls associated with the
            // repository.
            for url in urls(pkg) {
                timestamp_cache.insert(url.leak(), repo_status.leak_url());
            }
        }
        Ok(repo_status)
    })
}

fn timestamp_uncached(pkg: &Package) -> Result<RepoStatus<'_, SystemTime>> {
    if pkg.repository.is_none() {
        return Ok(RepoStatus::Unnamed);
    }

    timestamp_from_clone(pkg)
}

fn timestamp_from_clone(pkg: &Package) -> Result<RepoStatus<'_, SystemTime>> {
    let repo_status = clone_repository(pkg)?;

    let Some((url, repo_dir)) = repo_status.as_success() else {
        return Ok(repo_status.map_failure());
    };

    let mut command = Command::new("git");
    command
        .args(["log", "-1", "--pretty=format:%ct"])
        .current_dir(repo_dir);
    let output = command
        .output()
        .with_context(|| format!("failed to run command: {command:?}"))?;
    ensure!(output.status.success(), "command failed: {command:?}");

    let stdout = std::str::from_utf8(&output.stdout)?;
    let secs = u64::from_str(stdout.trim_end())?;
    let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);

    Ok(RepoStatus::Success(url, timestamp))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
#[cfg_attr(dylint_lib = "supplementary", allow(commented_out_code))]
fn clone_repository(pkg: &Package) -> Result<RepoStatus<PathBuf>> {
    let repo_status = REPOSITORY_CACHE.with_borrow_mut(|repository_cache| -> Result<_> {
        on_disk_cache::with_cache(|cache| -> Result<_> {
            // smoelius: Check all urls associated with the package.
            for url in urls(pkg) {
                if let Some(repo_status) = repository_cache.get(&url) {
                    return Ok(repo_status.clone());
                }
            }
            // smoelius: To make verbose printing easier, "membership" is printed regardless of the
            // check's purpose, and the `Purpose` type was removed.
            /* let what = match purpose {
                Purpose::Membership => "membership",
                Purpose::Timestamp => "timestamp",
            }; */
            verbose::wrap!(
                || {
                    let url_and_dir = cache.clone_repository(pkg);
                    match url_and_dir {
                        Ok((url_string, repo_dir)) => {
                            // smoelius: Note the use of `leak` in the next line. But the url is
                            // acting as a key in a global map, so it is not so bad.
                            let url = Url::from(url_string.as_str()).leak();
                            repository_cache
                                .insert(url, RepoStatus::Success(url, repo_dir.clone()).leak_url());
                            Ok(RepoStatus::Success(url, repo_dir))
                        }
                        Err(error) => {
                            let repo_status = if let Some(url_string) = &pkg.repository {
                                let url = url_string.as_str().into();
                                // smoelius: If cloning failed because the repository does not
                                // exist, adjust the repo status.
                                let existence = general_status(&pkg.name, url)?;
                                let repo_status = if existence.is_failure() {
                                    existence.map_failure()
                                } else {
                                    RepoStatus::Uncloneable(url)
                                };
                                warn!("failed to clone `{}`: {}", url_string, error);
                                repo_status
                            } else {
                                RepoStatus::Unnamed
                            };
                            // smoelius: In the event of failure, set all urls associated with
                            // the repository.
                            for url in urls(pkg) {
                                repository_cache.insert(url.leak(), repo_status.clone().leak_url());
                            }
                            Ok(repo_status)
                        }
                    }
                },
                RepoStatus::to_membership_string,
                "membership of `{}` using shallow clone",
                pkg.name
            )
        })
    })?;

    let Some((url, repo_dir)) = repo_status.as_success() else {
        return Ok(repo_status);
    };

    // smoelius: Even if `purpose` is `Purpose::Timestamp`, verify that `pkg` is a member of the
    // repository.
    if membership_in_clone(pkg, repo_dir)? {
        Ok(repo_status)
    } else {
        Ok(RepoStatus::Unassociated(url))
    }
}

const LINE_PREFIX: &str = "D  ";

fn membership_in_clone(pkg: &Package, repo_dir: &Path) -> Result<bool> {
    let mut command = Command::new("git");
    command.args(["status", "--porcelain"]);
    command.current_dir(repo_dir);
    command.stdout(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("command failed: {command:?}"))?;
    #[allow(clippy::unwrap_used)]
    let stdout = child.stdout.take().unwrap();
    let reader = std::io::BufReader::new(stdout);
    for result in reader.lines() {
        let line = result.with_context(|| format!("failed to read `{}`", repo_dir.display()))?;
        #[allow(clippy::panic)]
        let path = line.strip_prefix(LINE_PREFIX).map_or_else(
            || panic!("cache is corrupt at `{}`", repo_dir.display()),
            Path::new,
        );
        if path.file_name() != Some(OsStr::new("Cargo.toml")) {
            continue;
        }
        let contents = show(repo_dir, path)?;
        let Ok(table) = contents.parse::<Table>()
        /* smoelius: This "failed to parse" warning is a little too noisy.
        .map_err(|error| {
            warn!(
                "failed to parse {:?}: {}",
                path,
                error.to_string().trim_end()
            );
        }) */
        else {
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

fn show(repo_dir: &Path, path: &Path) -> Result<String> {
    let mut command = Command::new("git");
    command.args(["show", &format!("HEAD:{}", path.display())]);
    command.current_dir(repo_dir);
    command.stdout(Stdio::piped());
    let output = command
        .output()
        .with_context(|| format!("failed to run command: {command:?}"))?;
    if !output.status.success() {
        let error = String::from_utf8(output.stderr)?;
        bail!(
            "failed to read `{}` in `{}`: {}",
            path.display(),
            repo_dir.display(),
            error
        );
    }
    String::from_utf8(output.stdout).map_err(Into::into)
}

fn display_unmaintained_pkgs(unmaintained_pkgs: &[UnmaintainedPkg]) -> Result<()> {
    let mut pkgs_needing_warning = Vec::new();
    let mut at_least_one_newer_version_is_available = false;
    for unmaintained_pkg in unmaintained_pkgs {
        at_least_one_newer_version_is_available |= unmaintained_pkg.newer_version_is_available;
        if display_unmaintained_pkg(unmaintained_pkg)? {
            pkgs_needing_warning.push(unmaintained_pkg.pkg);
        }
    }
    if at_least_one_newer_version_is_available {
        println!(
            "\n* a newer (though still seemingly unmaintained) version of the package is available"
        );
    }
    if !pkgs_needing_warning.is_empty() {
        warn!(
            "the following packages' paths could not be printed:{}",
            pkgs_needing_warning
                .into_iter()
                .map(|pkg| format!("\n    {}@{}", pkg.name, pkg.version))
                .collect::<String>()
        );
    }
    Ok(())
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
#[cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]
fn display_unmaintained_pkg(unmaintained_pkg: &UnmaintainedPkg) -> Result<bool> {
    use std::io::Write;
    let mut stdout = StandardStream::stdout(opts::get().color);
    let UnmaintainedPkg {
        pkg,
        repo_age,
        newer_version_is_available,
        outdated_deps,
    } = unmaintained_pkg;
    stdout.set_color(ColorSpec::new().set_fg(repo_age.color()))?;
    write!(stdout, "{}", pkg.name)?;
    stdout.set_color(ColorSpec::new().set_fg(None))?;
    write!(stdout, " (")?;
    repo_age.write(&mut stdout)?;
    write!(stdout, ")")?;
    if *newer_version_is_available {
        write!(stdout, "*")?;
    }
    writeln!(stdout)?;
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
        let need_warning = display_path(&pkg.name, &pkg.version)?;
        println!();
        Ok(need_warning)
    } else {
        Ok(false)
    }
}

fn display_path(name: &str, version: &Version) -> Result<bool> {
    let spec = format!("{name}@{version}");
    let mut command = Command::new("cargo");
    command.args(["tree", "--workspace", "--target=all", "--invert", &spec]);
    let output = command
        .output()
        .with_context(|| format!("failed to run command: {command:?}"))?;
    // smoelius: Hack. It appears that `cargo tree` does not print proc-macros used by proc-macros.
    // For now, check whether stdout begins as expected. If not, ignore it and ultimately emit a
    // warning.
    let stdout = String::from_utf8(output.stdout)?;
    if stdout.split_ascii_whitespace().take(2).collect::<Vec<_>>() == [name, &format!("v{version}")]
    {
        print!("{stdout}");
        Ok(false)
    } else {
        Ok(true)
    }
}

static INDEX_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    #[allow(clippy::unwrap_used)]
    let cargo_home = cargo_home().unwrap();
    cargo_home.join("registry/index")
});

#[cfg(feature = "lock-index")]
fn lock_index() -> Result<File> {
    flock::lock_path(&INDEX_PATH)
        .with_context(|| format!("failed to lock `{}`", INDEX_PATH.display()))
}

#[cfg(not(feature = "lock-index"))]
fn lock_index() -> Result<File> {
    File::open(&*INDEX_PATH).with_context(|| format!("failed to open `{}`", INDEX_PATH.display()))
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

    #[test]
    fn repo_status_ord() {
        let ys = vec![
            RepoStatus::Uncloneable("f".into()),
            RepoStatus::Unnamed,
            RepoStatus::Success("e".into(), 0),
            RepoStatus::Success("d".into(), 1),
            RepoStatus::Unassociated("c".into()),
            RepoStatus::Nonexistent("b".into()),
            RepoStatus::Archived("a".into()),
        ];
        let mut xs = ys.clone();
        xs.sort_by_key(|repo_status| repo_status.erase_url());
        assert_eq!(xs, ys);
    }
}
