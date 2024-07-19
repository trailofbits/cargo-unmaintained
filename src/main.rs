#![deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use anyhow::{anyhow, bail, ensure, Context, Result};
use cargo_metadata::{
    semver::{Version, VersionReq},
    Dependency, DependencyKind, Metadata, MetadataCommand, Package,
};
use clap::{crate_version, Parser};
use crates_index::GitIndex;
use home::cargo_home;
use once_cell::{sync::Lazy, unsync::OnceCell};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env::args,
    ffi::OsStr,
    fs::{read_to_string, File},
    path::{Path, PathBuf},
    process::{exit, Command},
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime},
};
use tempfile::TempDir;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use toml::{Table, Value};
use walkdir::WalkDir;

mod curl;
mod flush;
mod github;
mod on_disk_cache;
mod opts;
mod packaging;
mod url;
mod verbose;

#[cfg(feature = "lock-index")]
mod flock;

use url::{urls, Url};

const SECS_PER_DAY: u64 = 24 * 60 * 60;

const DEFAULT_REFRESH_AGE: u64 = 30; // days

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
    about = "Find unmaintained packages in Rust projects",
    after_help = "\
The `GITHUB_TOKEN_PATH` environment variable can be set to the path of a file containing a \
personal access token. If set, cargo-unmaintained will use this token to authenticate to GitHub \
and check whether packages' repositories have been archived.

Unless --no-exit-code is passed, the exit status is 0 if no unmaintained packages were found and \
no irrecoverable errors occurred, 1 if unmaintained packages were found, and 2 if an irrecoverable \
error occurred."
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
    #[clap(long, help = "Do not save cloned repositories on disk for future runs")]
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

    #[clap(long, help = "Show paths to unmaintained packages")]
    tree: bool,

    #[clap(long, help = "Show information about what cargo-unmaintained is doing")]
    verbose: bool,
}

struct UnmaintainedPkg<'a> {
    pkg: &'a Package,
    repo_age: RepoStatus<'a, u64>,
    outdated_deps: Vec<OutdatedDep<'a>>,
}

/// Repository statuses with the variants ordered by how "bad" they are.
///
/// A `RepoStatus` has a url only if it's not `Unnamed`. A `RepoStatus` has a value only if
/// it is `Success`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RepoStatus<'a, T> {
    Uncloneable(Url<'a>),
    Unnamed,
    Success(Url<'a>, T),
    Unassociated(Url<'a>),
    Nonexistent(Url<'a>),
    Archived(Url<'a>),
}

impl<'a, T> RepoStatus<'a, T> {
    fn as_success(&self) -> Option<(Url<'a>, &T)> {
        match self {
            Self::Uncloneable(_)
            | Self::Unnamed
            | Self::Unassociated(_)
            | Self::Nonexistent(_)
            | Self::Archived(_) => None,
            Self::Success(url, value) => Some((*url, value)),
        }
    }

    #[allow(dead_code)]
    fn is_success(&self) -> bool {
        self.as_success().is_some()
    }

    fn is_failure(&self) -> bool {
        self.as_success().is_none()
    }

    fn erase_url(self) -> RepoStatus<'static, T> {
        match self {
            Self::Uncloneable(_) => RepoStatus::Uncloneable(Url::default()),
            Self::Unnamed => RepoStatus::Unnamed,
            Self::Success(_, value) => RepoStatus::Success(Url::default(), value),
            Self::Unassociated(_) => RepoStatus::Unassociated(Url::default()),
            Self::Nonexistent(_) => RepoStatus::Nonexistent(Url::default()),
            Self::Archived(_) => RepoStatus::Archived(Url::default()),
        }
    }

    // smoelius: This isn't as bad as it looks. `leak_url` is used only when a `RepoStatus` needs to
    // be inserted into a global data structure. In such a case, the `RepoStatus`'s drop handler
    // would be called either never or when the program terminates. So the effect of leaking the url
    // is rather insignificant.
    fn leak_url(self) -> RepoStatus<'static, T> {
        match self {
            Self::Uncloneable(url) => RepoStatus::Uncloneable(url.leak()),
            Self::Unnamed => RepoStatus::Unnamed,
            Self::Success(url, value) => RepoStatus::Success(url.leak(), value),
            Self::Unassociated(url) => RepoStatus::Unassociated(url.leak()),
            Self::Nonexistent(url) => RepoStatus::Nonexistent(url.leak()),
            Self::Archived(url) => RepoStatus::Archived(url.leak()),
        }
    }

    fn map<U>(self, f: impl Fn(T) -> U) -> RepoStatus<'a, U> {
        match self {
            Self::Uncloneable(url) => RepoStatus::Uncloneable(url),
            Self::Unnamed => RepoStatus::Unnamed,
            Self::Success(url, value) => RepoStatus::Success(url, f(value)),
            Self::Unassociated(url) => RepoStatus::Unassociated(url),
            Self::Nonexistent(url) => RepoStatus::Nonexistent(url),
            Self::Archived(url) => RepoStatus::Archived(url),
        }
    }

    #[allow(clippy::panic)]
    fn map_failure<U>(self) -> RepoStatus<'a, U> {
        self.map(|_| panic!("unexpected `RepoStatus::Success`"))
    }
}

impl<'a, T, E> RepoStatus<'a, Result<T, E>> {
    fn transpose(self) -> Result<RepoStatus<'a, T>, E> {
        match self {
            Self::Uncloneable(url) => Ok(RepoStatus::Uncloneable(url)),
            Self::Unnamed => Ok(RepoStatus::Unnamed),
            Self::Success(url, Ok(value)) => Ok(RepoStatus::Success(url, value)),
            Self::Success(_, Err(error)) => Err(error),
            Self::Unassociated(url) => Ok(RepoStatus::Unassociated(url)),
            Self::Nonexistent(url) => Ok(RepoStatus::Nonexistent(url)),
            Self::Archived(url) => Ok(RepoStatus::Archived(url)),
        }
    }
}

/// Multiples of `max_age` that cause the color to go completely from yellow to red.
const SATURATION_MULTIPLIER: u64 = 3;

impl<'a> RepoStatus<'a, u64> {
    fn color(&self) -> Option<Color> {
        let age = match self {
            // smoelius: `Uncloneable` and `Unnamed` default to yellow.
            Self::Uncloneable(_) | Self::Unnamed => {
                return Some(Color::Rgb(u8::MAX, u8::MAX, 0));
            }
            Self::Success(_, age) => age,
            // smoelius: `Unassociated`, `Nonexistent`, and `Archived` default to red.
            Self::Unassociated(_) | Self::Nonexistent(_) | Self::Archived(_) => {
                return Some(Color::Rgb(u8::MAX, 0, 0));
            }
        };
        let age_in_days = age / SECS_PER_DAY;
        let Some(max_age_excess) = age_in_days.checked_sub(opts::get().max_age) else {
            // smoelius: `age_in_days` should be at least `max_age`. Otherwise, why are we here?
            debug_assert!(false);
            return None;
        };
        let subtrahend_u64 = if opts::get().max_age == 0 {
            u64::MAX
        } else {
            (max_age_excess * u64::from(u8::MAX)) / (SATURATION_MULTIPLIER * opts::get().max_age)
        };
        Some(Color::Rgb(
            u8::MAX,
            u8::MAX.saturating_sub(u8::try_from(subtrahend_u64).unwrap_or(u8::MAX)),
            0,
        ))
    }

    #[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
    #[cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]
    fn write(&self, stream: &mut impl WriteColor) -> std::io::Result<()> {
        match self {
            Self::Uncloneable(url) => {
                write_url(stream, *url)?;
                write!(stream, " is uncloneable")?;
                Ok(())
            }
            Self::Unnamed => write!(stream, "no repository"),
            Self::Success(url, age) => {
                write_url(stream, *url)?;
                write!(stream, " updated ")?;
                stream.set_color(ColorSpec::new().set_fg(self.color()))?;
                write!(stream, "{}", age / SECS_PER_DAY)?;
                stream.set_color(ColorSpec::new().set_fg(None))?;
                write!(stream, " days ago")?;
                Ok(())
            }
            Self::Unassociated(url) => {
                write!(stream, "not in ")?;
                write_url(stream, *url)?;
                Ok(())
            }
            Self::Nonexistent(url) => {
                write_url(stream, *url)?;
                write!(stream, " does not exist")?;
                Ok(())
            }
            Self::Archived(url) => {
                write_url(stream, *url)?;
                write!(stream, " archived")?;
                Ok(())
            }
        }
    }
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
#[cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]
fn write_url(stream: &mut impl WriteColor, url: Url) -> std::io::Result<()> {
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Blue)))?;
    write!(stream, "{url}")?;
    stream.set_color(ColorSpec::new().set_fg(None))?;
    Ok(())
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
            eprintln!(concat!("warning: ", $fmt), $($arg)*);
        }
    };
}

thread_local! {
    #[allow(clippy::unwrap_used)]
    static INDEX: Lazy<GitIndex> = Lazy::new(|| {
        let _lock = lock_index().unwrap();
        let mut index = GitIndex::new_cargo_default().unwrap();
        if let Err(error) = index.update() {
            warn!("failed to update index: {}", error);
        }
        index
    });
    // smoelius: The next four statics are "in-memory" caches.
    static GENERAL_STATUS_CACHE: RefCell<HashMap<Url<'static>, RepoStatus<'static, ()>>> = RefCell::new(HashMap::new());
    static LATEST_VERSION_CACHE: RefCell<HashMap<String, Version>> = RefCell::new(HashMap::new());
    static TIMESTAMP_CACHE: RefCell<HashMap<Url<'static>, RepoStatus<'static, SystemTime>>> = RefCell::new(HashMap::new());
    static REPOSITORY_CACHE: RefCell<HashMap<Url<'static>, RepoStatus<'static, PathBuf>>> = RefCell::new(HashMap::new());
    // smoelius: The next static is an "on-disk" cache that resides at
    // `$HOME/.cache/cargo-unmaintained/v1`.
    static ON_DISK_CACHE_ONCE_CELL: RefCell<OnceCell<on_disk_cache::Cache>> = const { RefCell::new(OnceCell::new()) };
}

static TOKEN_FOUND: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    env_logger::init();

    let Cargo {
        subcmd: CargoSubCommand::Unmaintained(opts),
    } = Cargo::parse_from(args());

    opts::init(opts);

    if github::load_token(|_| Ok(()))? {
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

    for pkg in packages {
        if let Some(unmaintained_pkg) = is_unmaintained_package(&metadata, pkg)? {
            unmaintained_pkgs.push(unmaintained_pkg);

            if opts::get().fail_fast {
                break;
            }
        }
    }

    if unmaintained_pkgs.is_empty() {
        eprintln!("No unmaintained packages found");
        return Ok(false);
    }

    unmaintained_pkgs.sort_by_key(|unmaintained| unmaintained.repo_age.erase_url());

    display_unmaintained_pkgs(&unmaintained_pkgs)?;

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

fn is_unmaintained_package<'a>(
    metadata: &'a Metadata,
    pkg: &'a Package,
) -> Result<Option<UnmaintainedPkg<'a>>> {
    if let Some(url_string) = &pkg.repository {
        let can_use_github_api =
            TOKEN_FOUND.load(Ordering::SeqCst) && url_string.starts_with("https://github.com/");

        if can_use_github_api {
            let repo_status = general_status(&pkg.name, url_string.as_str().into())?;
            if repo_status.is_failure() {
                return Ok(Some(UnmaintainedPkg {
                    pkg,
                    repo_age: repo_status.map_failure(),
                    outdated_deps: Vec::new(),
                }));
            }
        }

        let repo_status = clone_repository(pkg, Purpose::Membership)?;
        if repo_status.is_failure() {
            return Ok(Some(UnmaintainedPkg {
                pkg,
                repo_age: repo_status.map_failure(),
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
        .map_or(false, |(_, &age)| age < opts::get().max_age * SECS_PER_DAY)
    {
        return Ok(None);
    }

    Ok(Some(UnmaintainedPkg {
        pkg,
        repo_age,
        outdated_deps,
    }))
}

fn general_status(name: &str, url: Url) -> Result<RepoStatus<'static, ()>> {
    GENERAL_STATUS_CACHE.with_borrow_mut(|general_status_cache| {
        if let Some(&value) = general_status_cache.get(&url) {
            return Ok(value);
        }
        let (use_github_api, what, how) = if TOKEN_FOUND.load(Ordering::SeqCst)
            && url.as_str().starts_with("https://github.com/")
        {
            (true, "archival status", "GitHub API")
        } else {
            (false, "existence", "HTTP request")
        };
        verbose::wrap!(
            || {
                let repo_status = if use_github_api {
                    github::archival_status(url)
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
                let repo_status = clone_repository(pkg, Purpose::Membership)?;
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
    };

    timestamp_from_clone(pkg)
}

fn timestamp_from_clone(pkg: &Package) -> Result<RepoStatus<'_, SystemTime>> {
    let repo_status = clone_repository(pkg, Purpose::Timestamp)?;

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

#[derive(Clone, Copy)]
enum Purpose {
    /// Verify a package is a member of the repository
    Membership,
    /// Determine the repository's timestamp
    Timestamp,
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn clone_repository(pkg: &Package, purpose: Purpose) -> Result<RepoStatus<PathBuf>> {
    let repo_status = REPOSITORY_CACHE.with_borrow_mut(|repository_cache| -> Result<_> {
        ON_DISK_CACHE_ONCE_CELL.with_borrow_mut(|once_cell| -> Result<_> {
            // smoelius: Check all urls associated with the package.
            for url in urls(pkg) {
                if let Some(repo_status) = repository_cache.get(&url) {
                    return Ok(repo_status.clone());
                }
            }
            let what = match purpose {
                Purpose::Membership => "membership",
                Purpose::Timestamp => "timestamp",
            };
            verbose::wrap!(
                || {
                    let url_and_dir = on_disk_cache(once_cell).clone_repository(pkg);
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
                "{} of `{}` using shallow clone",
                what,
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

fn on_disk_cache(once_cell: &mut OnceCell<on_disk_cache::Cache>) -> &mut on_disk_cache::Cache {
    let _: &on_disk_cache::Cache = once_cell.get_or_init(|| {
        #[cfg(all(feature = "on-disk-cache", not(windows)))]
        let temporary = opts::get().no_cache;

        #[cfg(any(not(feature = "on-disk-cache"), windows))]
        let temporary = true;

        #[allow(clippy::panic)]
        on_disk_cache::Cache::new(
            temporary,
            std::cmp::min(DEFAULT_REFRESH_AGE, opts::get().max_age),
        )
        .unwrap_or_else(|error| panic!("failed to create on-disk repository cache: {error}"))
    });

    #[allow(clippy::unwrap_used)]
    once_cell.get_mut().unwrap()
}

fn membership_in_clone(pkg: &Package, repo_dir: &Path) -> Result<bool> {
    for entry in WalkDir::new(repo_dir) {
        let entry = entry?;
        let path = entry.path();
        if path.file_name() != Some(OsStr::new("Cargo.toml")) {
            continue;
        }
        let contents = read_to_string(path).with_context(|| format!("failed to read {path:?}"))?;
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

fn display_unmaintained_pkgs(unmaintained_pkgs: &[UnmaintainedPkg]) -> Result<()> {
    let mut pkgs_needing_warning = Vec::new();
    for unmaintained_pkg in unmaintained_pkgs {
        if display_unmaintained_pkg(unmaintained_pkg)? {
            pkgs_needing_warning.push(&unmaintained_pkg.pkg);
        }
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
        outdated_deps,
    } = unmaintained_pkg;
    stdout.set_color(ColorSpec::new().set_fg(repo_age.color()))?;
    write!(stdout, "{}", pkg.name)?;
    stdout.set_color(ColorSpec::new().set_fg(None))?;
    write!(stdout, " (")?;
    repo_age.write(&mut stdout)?;
    writeln!(stdout, ")")?;
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

static INDEX_PATH: Lazy<PathBuf> = Lazy::new(|| {
    #[allow(clippy::unwrap_used)]
    let cargo_home = cargo_home().unwrap();
    cargo_home.join("registry/index")
});

#[cfg(feature = "lock-index")]
fn lock_index() -> Result<File> {
    flock::lock_path(&INDEX_PATH).with_context(|| format!("failed to lock {:?}", &*INDEX_PATH))
}

#[cfg(not(feature = "lock-index"))]
fn lock_index() -> Result<File> {
    File::open(&*INDEX_PATH).with_context(|| format!("failed to open {:?}", &*INDEX_PATH))
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
