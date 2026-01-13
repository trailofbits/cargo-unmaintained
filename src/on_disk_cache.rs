//! On-disk cache
//!
//! The on-disk cache consists of the following subdirectories:
//! - `entries`: JSON-encoded [`Entry`]. Each file's name is the associated package's name.
//! - `repositories`: Cloned repositories. Each subdirectory's name is the hash of the url that was
//!   cloned.
//! - `timestamps`: Number of seconds between the Unix epoch and the time when the repository was
//!   cloned. Filenames are the same as those of the cloned repositories.
//! - `versions`: JSON-encoded array of [`crates_io_api::Version`]. Each file's name is the
//!   associated package's name.
//! - `versions_timestamps`: Number of seconds between the Unix epoch and the time when the versions
//!   were fetched. Filenames are the same as those of the fetched versions.
//!
//! A package's entry is considered current if both of the following conditions are met:
//! - A url associated with the package was successfully cloned.
//! - The clone was performed no more than `refresh_age` days ago.
//!
//! If either of the above conditions are not met, an attempt is made to refresh the entry.
//!
//! A similar statement applies to versions.
//!
//! The on-disk cache resides at `$HOME/.cache/cargo-unmaintained/v2`.

use super::{SECS_PER_DAY, USER_AGENT, urls};
use anyhow::{Context, Result, anyhow, bail, ensure};
use cargo_metadata::{Package, PackageName};
use crates_io_api::{SyncClient, Version};
use elaborate::std::{
    fs::{create_dir_all_wc, read_to_string_wc, remove_dir_all_wc, remove_file_wc, write_wc},
    path::PathContext,
    process::CommandContext,
    time::SystemTimeContext,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    sync::LazyLock,
    time::{Duration, SystemTime},
};
use tempfile::{TempDir, tempdir};

const DEFAULT_REFRESH_AGE: u64 = 30; // days

const RATE_LIMIT: Duration = Duration::from_secs(1);

#[derive(Clone, Deserialize, Serialize)]
struct Entry {
    named_url: String,
    cloned_url: String,
}

pub(crate) struct Cache {
    tempdir: Option<TempDir>,
    refresh_age: u64, // days
    entries: HashMap<PackageName, Entry>,
    repository_timestamps: HashMap<String, SystemTime>,
    versions_with_timestamps: HashMap<String, (Vec<Version>, SystemTime)>,
}

thread_local! {
    static CACHE_ONCE_CELL: RefCell<OnceCell<Cache>> = const { RefCell::new(OnceCell::new()) };
}

#[cfg(all(feature = "on-disk-cache", not(windows)))]
#[allow(clippy::unwrap_used)]
static CACHE_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    elaborate::std::env::var_wc("CARGO_UNMAINTAINED_CACHE").map_or_else(
        |_| {
            let base_directories = xdg::BaseDirectories::new();
            base_directories
                .create_cache_directory("cargo-unmaintained")
                .unwrap()
        },
        PathBuf::from,
    )
});

#[cfg(all(feature = "on-disk-cache", not(windows)))]
/// The current version of the cache structure
const VERSION: &str = "v2";

#[allow(clippy::unwrap_used)]
static CRATES_IO_SYNC_CLIENT: LazyLock<SyncClient> =
    LazyLock::new(|| SyncClient::new(USER_AGENT, RATE_LIMIT).unwrap());

pub fn with_cache<T>(f: impl FnOnce(&mut Cache) -> T) -> T {
    CACHE_ONCE_CELL.with_borrow_mut(|once_cell| {
        let _: &Cache = once_cell.get_or_init(|| {
            #[cfg(all(feature = "on-disk-cache", not(windows)))]
            let temporary = crate::opts::get().no_cache;

            #[cfg(any(not(feature = "on-disk-cache"), windows))]
            let temporary = true;

            #[allow(clippy::panic)]
            Cache::new(
                temporary,
                std::cmp::min(DEFAULT_REFRESH_AGE, crate::opts::get().max_age),
            )
            .unwrap_or_else(|error| panic!("failed to create on-disk repository cache: {error}"))
        });

        #[allow(clippy::unwrap_used)]
        let cache = once_cell.get_mut().unwrap();

        f(cache)
    })
}

impl Cache {
    fn new(temporary: bool, refresh_age: u64) -> Result<Self> {
        let tempdir = if temporary {
            tempdir()
                .map(Option::Some)
                .with_context(|| "failed to create temporary directory")?
        } else {
            None
        };
        Ok(Self {
            tempdir,
            refresh_age,
            entries: HashMap::new(),
            repository_timestamps: HashMap::new(),
            versions_with_timestamps: HashMap::new(),
        })
    }

    #[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
    pub fn clone_repository(&mut self, pkg: &Package) -> Result<(String, PathBuf)> {
        // smoelius: Ignore any errors that may occur while reading/deserializing.
        if let Ok(entry) = self.entry(pkg)
            && self
                .repository_is_current(&entry.cloned_url)
                .unwrap_or_default()
        {
            let repo_dir = self.repositories_dir().join(url_digest(&entry.cloned_url));
            return Ok((entry.cloned_url, repo_dir));
        }

        let url_and_dir = self.clone_repository_uncached(pkg)?;

        #[allow(clippy::unwrap_used)]
        let entry = Entry {
            named_url: pkg.repository.clone().unwrap(),
            cloned_url: url_and_dir.0.as_str().to_owned(),
        };
        self.write_entry(&pkg.name, &entry)?;
        self.entries.insert(pkg.name.clone(), entry);

        let digest = url_digest(&url_and_dir.0);
        let timestamp = SystemTime::now();
        self.write_repository_timestamp(&digest, timestamp)?;
        self.repository_timestamps.insert(digest, timestamp);

        Ok(url_and_dir)
    }

    fn clone_repository_uncached(&mut self, pkg: &Package) -> Result<(String, PathBuf)> {
        let mut errors = Vec::new();
        let mut urls = urls(pkg).into_iter().peekable();
        while let Some(url) = urls.peek() {
            let repo_dir = self.repositories_dir().join(url_digest(url.as_str()));
            let exists = repository_existence(&repo_dir)?;
            let mut command = if exists {
                let branch_name = branch_name(&repo_dir)?;
                let mut command = Command::new("git");
                command.args([
                    "fetch",
                    "--update-head-ok",
                    "origin",
                    &format!("{branch_name}:{branch_name}"),
                ]);
                command.current_dir(&repo_dir);
                command
            } else {
                let mut command = Command::new("git");
                // smoelius: The full repository is no longer checked out.
                command.args([
                    "clone",
                    "--depth=1",
                    "--no-checkout",
                    "--quiet",
                    url.as_str(),
                    &repo_dir.to_string_lossy(),
                ]);
                command
            };
            command
                .env("GCM_INTERACTIVE", "never")
                .env("GIT_ASKPASS", "echo")
                .env("GIT_TERMINAL_PROMPT", "0")
                .stderr(Stdio::piped());
            let output = command.output_wc()?;
            if output.status.success() {
                return Ok((url.as_str().to_owned(), repo_dir));
            }
            let error = String::from_utf8(output.stderr)?;
            if exists && error.starts_with("fatal: couldn't find remote ref ") {
                self.purge_one_entry(pkg)?;
                // smoelius: Verify repository no longer exists and retry with same url.
                debug_assert!(!repository_existence(&repo_dir)?);
                continue;
            }
            errors.push(error);
            let _: Option<crate::url::Url> = urls.next();
        }
        // smoelius: Don't emit duplicate errors.
        errors.dedup();
        Err(anyhow!("{errors:#?}"))
    }

    pub fn purge_one_entry(&mut self, pkg: &Package) -> Result<()> {
        // smoelius: `versions` and `versions_timestamps` will exist only if `fetch_versions` was
        // called.
        if self
            .versions_with_timestamps
            .contains_key(pkg.name.as_str())
        {
            let path_buf = self.versions_dir().join(pkg.name.as_str());
            remove_file_wc(&path_buf)?;

            let path_buf = self.versions_timestamps_dir().join(pkg.name.as_str());
            remove_file_wc(&path_buf)?;

            self.versions_with_timestamps.remove(pkg.name.as_str());
        }

        let entry = self.entry(pkg)?;
        let digest = url_digest(&entry.cloned_url);

        let path_buf = self.repository_timestamps_dir().join(&digest);
        remove_file_wc(&path_buf)?;
        self.repository_timestamps.remove(&digest);

        let path_buf = self.entries_dir().join(pkg.name.as_str());
        remove_file_wc(&path_buf)?;
        self.entries.remove(&pkg.name);

        // smoelius: Finally, remove the cloned repository.
        let path_buf = self.repositories_dir().join(digest);
        remove_dir_all_wc(&path_buf)?;

        Ok(())
    }

    fn entry(&mut self, pkg: &Package) -> Result<Entry> {
        if !self.entries.contains_key(&pkg.name) {
            let path_buf = self.entries_dir().join(pkg.name.as_str());
            let contents = read_to_string_wc(&path_buf)?;
            let entry = serde_json::from_str::<Entry>(&contents)?;
            ensure!(
                pkg.repository.as_ref() == Some(&entry.named_url),
                "`pkg.repository` and `entry.named_url` differ"
            );
            self.entries.insert(pkg.name.clone(), entry);
        }
        #[allow(clippy::unwrap_used)]
        Ok(self.entries.get(&pkg.name).cloned().unwrap())
    }

    fn repository_is_current(&mut self, url: &str) -> Result<bool> {
        self.repository_timestamp(url).and_then(|timestamp| {
            let duration = SystemTime::now().duration_since_wc(timestamp)?;
            Ok(duration.as_secs() < self.refresh_age * SECS_PER_DAY)
        })
    }

    fn repository_timestamp(&mut self, url: &str) -> Result<SystemTime> {
        let digest = url_digest(url);
        if !self.repository_timestamps.contains_key(&digest) {
            let path_buf = self.repository_timestamps_dir().join(url_digest(url));
            let contents = read_to_string_wc(&path_buf)?;
            let secs = u64::from_str(&contents)?;
            let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
            self.repository_timestamps.insert(digest.clone(), timestamp);
        }
        #[allow(clippy::unwrap_used)]
        Ok(*self.repository_timestamps.get(&digest).unwrap())
    }

    pub fn fetch_versions(&mut self, name: &str) -> Result<Vec<Version>> {
        // smoelius: Ignore any errors that may occur while reading/deserializing.
        if let Ok(versions) = self.versions(name)
            && self.versions_are_current(name).unwrap_or_default()
        {
            return Ok(versions);
        }

        let crate_response = CRATES_IO_SYNC_CLIENT.get_crate(name)?;
        // smoelius: Avoid using anything other than `versions` from `CrateResponse`. In particular,
        // avoid using `crate_data`. The same data should be available in the crates.io index.
        let versions = crate_response.versions;
        self.write_versions(name, &versions)?;

        let timestamp = SystemTime::now();
        self.write_versions_timestamp(name, timestamp)?;

        self.versions_with_timestamps
            .insert(name.to_owned(), (versions.clone(), timestamp));

        Ok(versions)
    }

    fn versions(&mut self, name: &str) -> Result<Vec<Version>> {
        self.versions_with_timestamp(name)
            .map(|(versions, _)| versions)
    }

    fn versions_are_current(&mut self, url: &str) -> Result<bool> {
        self.versions_timestamp(url).and_then(|timestamp| {
            let duration = SystemTime::now().duration_since_wc(timestamp)?;
            Ok(duration.as_secs() < self.refresh_age * SECS_PER_DAY)
        })
    }

    fn versions_timestamp(&mut self, name: &str) -> Result<SystemTime> {
        self.versions_with_timestamp(name)
            .map(|(_, timestamp)| timestamp)
    }

    fn versions_with_timestamp(&mut self, name: &str) -> Result<(Vec<Version>, SystemTime)> {
        if !self.versions_with_timestamps.contains_key(name) {
            let path_buf = self.versions_timestamps_dir().join(name);
            let contents = read_to_string_wc(&path_buf)?;
            let versions = serde_json::from_str::<Vec<Version>>(&contents)?;
            self.versions_with_timestamps
                .insert(name.to_owned(), (versions, SystemTime::now()));
        }
        #[allow(clippy::unwrap_used)]
        Ok(self.versions_with_timestamps.get(name).cloned().unwrap())
    }

    fn write_entry(&self, pkg_name: &str, entry: &Entry) -> Result<()> {
        create_dir_all_wc(self.entries_dir())?;
        let path_buf = self.entries_dir().join(pkg_name);
        let json = serde_json::to_string_pretty(entry)?;
        write_wc(&path_buf, json)?;
        Ok(())
    }

    fn write_repository_timestamp(&self, digest: &str, timestamp: SystemTime) -> Result<()> {
        create_dir_all_wc(self.repository_timestamps_dir())?;
        let path_buf = self.repository_timestamps_dir().join(digest);
        let duration = timestamp.duration_since_wc(SystemTime::UNIX_EPOCH)?;
        write_wc(&path_buf, duration.as_secs().to_string())?;
        Ok(())
    }

    fn write_versions(&self, name: &str, versions: &[Version]) -> Result<()> {
        create_dir_all_wc(self.versions_dir())?;
        let path_buf = self.versions_dir().join(name);
        let json = serde_json::to_string_pretty(versions)?;
        write_wc(&path_buf, json)?;
        Ok(())
    }

    fn write_versions_timestamp(&self, name: &str, timestamp: SystemTime) -> Result<()> {
        create_dir_all_wc(self.versions_timestamps_dir())?;
        let path_buf = self.versions_timestamps_dir().join(name);
        let duration = timestamp.duration_since_wc(SystemTime::UNIX_EPOCH)?;
        write_wc(&path_buf, duration.as_secs().to_string())?;
        Ok(())
    }

    fn entries_dir(&self) -> PathBuf {
        self.base_dir().join("entries")
    }

    fn repositories_dir(&self) -> PathBuf {
        self.base_dir().join("repositories")
    }

    // smoelius: FIXME: Rename this directory to "repository_timestamps".
    fn repository_timestamps_dir(&self) -> PathBuf {
        self.base_dir().join("timestamps")
    }

    fn versions_dir(&self) -> PathBuf {
        self.base_dir().join("versions")
    }

    fn versions_timestamps_dir(&self) -> PathBuf {
        self.base_dir().join("versions_timestamps")
    }

    fn base_dir(&self) -> PathBuf {
        let base_dir = self.tempdir.as_ref().map(TempDir::path);

        #[cfg(all(feature = "on-disk-cache", not(windows)))]
        {
            base_dir.unwrap_or(&CACHE_DIRECTORY).join(VERSION)
        }

        #[cfg(any(not(feature = "on-disk-cache"), windows))]
        #[allow(clippy::unwrap_used)]
        base_dir.unwrap().to_path_buf()
    }
}

fn url_digest(url: &str) -> String {
    sha1_smol::Sha1::from(url).hexdigest()
}

fn repository_existence(repo_dir: &Path) -> Result<bool> {
    repo_dir.try_exists_wc()
}

fn branch_name(repo_dir: &Path) -> Result<String> {
    let mut command = Command::new("git");
    command.args(["branch", "--show-current"]);
    command.current_dir(repo_dir);
    let output = command.output_wc()?;
    if !output.status.success() {
        let error = String::from_utf8(output.stderr)?;
        bail!(
            "failed to get `{}` branch name: {}",
            repo_dir.display(),
            error
        );
    }
    let stdout = std::str::from_utf8(&output.stdout)?;
    Ok(stdout.trim_end().to_owned())
}

/// Purges the on-disk cache directory.
///
/// It removes the entire cache directory at $HOME/.cache/cargo-unmaintained.
#[cfg(all(feature = "on-disk-cache", not(windows)))]
pub fn purge_cache() -> Result<()> {
    if CACHE_DIRECTORY.try_exists_wc()? {
        // Remove the entire cache directory
        remove_dir_all_wc(&*CACHE_DIRECTORY)?;

        eprintln!("Cache directory removed: {}", CACHE_DIRECTORY.display());
    } else {
        eprintln!(
            "Cache directory does not exist: {}",
            CACHE_DIRECTORY.display()
        );
    }

    Ok(())
}

#[cfg(all(test, not(windows)))]
mod tests {
    // smoelius: The clippy.toml that the `elaborate_disallowed_methods` test uses doesn't have
    // `allow-unwrap-in-tests = true`.
    #![allow(clippy::unwrap_used)]

    use super::{Cache, DEFAULT_REFRESH_AGE};
    use cargo_metadata::MetadataCommand;

    #[test]
    fn purge_one_entry() {
        let mut cache = Cache::new(false, DEFAULT_REFRESH_AGE).unwrap();
        let metadata = MetadataCommand::new().exec().unwrap();
        // smoelius: Any package will do. Use the first one that `cargo-unmaintained` relies on.
        let pkg = metadata.packages.first().unwrap();
        cache.clone_repository(pkg).unwrap();
        // smoelius: Ensure `versions` and `version_timestamps` exist.
        cache.fetch_versions(&pkg.name).unwrap();
        cache.purge_one_entry(pkg).unwrap();
    }
}
