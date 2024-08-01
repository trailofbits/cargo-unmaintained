//! On-disk cache
//!
//! The on-disk cache consists of the following subdirectories:
//! - `entries`: JSON-encoded [`Entry`]. Each file's name is that of package with which it is
//!   associated.
//! - `repositories`: Cloned repositories. Each subdirectory's name is the hash of the url that was
//!   cloned.
//! - `timestamps`: Number of seconds between the Unix epoch and the time when the repository was
//!   cloned. Filenames are the same as those of the cloned repositories.
//! - `versions`: JSON-encoded array of [`crates_io_api::Version`]. Each file's name is that of
//!   package with which it is associated.
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

use super::{urls, SECS_PER_DAY};
use anyhow::{anyhow, ensure, Context, Result};
use cargo_metadata::Package;
use crates_io_api::{SyncClient, Version};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{create_dir_all, read_to_string, write, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    time::{Duration, SystemTime},
};
use tempfile::{tempdir, TempDir};

const USER_AGENT: &str = "cargo-unmaintained (github.com/trailofbits/cargo-unmaintained)";

const RATE_LIMIT: Duration = Duration::from_secs(1);

#[derive(Clone, Deserialize, Serialize)]
struct Entry {
    named_url: String,
    cloned_url: String,
}

pub(crate) struct Cache {
    tempdir: Option<TempDir>,
    refresh_age: u64, // days
    entries: HashMap<String, Entry>,
    repository_timestamps: HashMap<String, SystemTime>,
    versions: HashMap<String, Vec<Version>>,
    versions_timestamps: HashMap<String, SystemTime>,
}

#[cfg(all(feature = "on-disk-cache", not(windows)))]
#[allow(clippy::unwrap_used)]
static CACHE_DIRECTORY: Lazy<PathBuf> = Lazy::new(|| {
    let base_directories = xdg::BaseDirectories::new().unwrap();
    base_directories
        .create_cache_directory("cargo-unmaintained/v1")
        .unwrap()
});

#[allow(clippy::unwrap_used)]
static CRATES_IO_SYNC_CLIENT: Lazy<SyncClient> =
    Lazy::new(|| SyncClient::new(USER_AGENT, RATE_LIMIT).unwrap());

impl Cache {
    pub fn new(temporary: bool, refresh_age: u64) -> Result<Self> {
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
            versions: HashMap::new(),
            versions_timestamps: HashMap::new(),
        })
    }

    #[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
    pub fn clone_repository(&mut self, pkg: &Package) -> Result<(String, PathBuf)> {
        // smoelius: Ignore any errors that may occur while reading/deserializing.
        if let Ok(entry) = self.entry(pkg) {
            if self
                .repository_is_current(&entry.cloned_url)
                .unwrap_or_default()
            {
                let repo_dir = self.repositories_dir().join(url_digest(&entry.cloned_url));
                return Ok((entry.cloned_url, repo_dir));
            }
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

    fn clone_repository_uncached(&self, pkg: &Package) -> Result<(String, PathBuf)> {
        // smoelius: The next `lock_path` locks the entire cache. This is needed for the `snapbox`
        // tests, because they run concurrently. I am not sure how much contention this locking
        // causes.
        let _lock: File;
        #[cfg(all(feature = "on-disk-cache", feature = "lock-index", not(windows)))]
        if self.tempdir.is_none() {
            _lock = crate::flock::lock_path(&CACHE_DIRECTORY)
                .with_context(|| format!("failed to lock {:?}", &*CACHE_DIRECTORY))?;
        }

        let mut errors = Vec::new();
        for url in urls(pkg) {
            let repo_dir = self.repositories_dir().join(url_digest(url.as_str()));
            let exists = repository_existence(&repo_dir)?;
            let mut command = if exists {
                let mut command = Command::new("git");
                command.args(["pull", "--ff-only"]);
                command.current_dir(&repo_dir);
                command
            } else {
                let mut command = Command::new("git");
                command.args([
                    "clone",
                    "--depth=1",
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
            let output = command
                .output()
                .with_context(|| format!("failed to run command: {command:?}"))?;
            if output.status.success() {
                return Ok((url.as_str().to_owned(), repo_dir));
            }
            let error = String::from_utf8(output.stderr)?;
            errors.push(error);
        }
        // smoelius: Don't emit duplicate errors.
        errors.dedup();
        Err(anyhow!("{:#?}", errors))
    }

    fn entry(&mut self, pkg: &Package) -> Result<Entry> {
        if !self.entries.contains_key(&pkg.name) {
            let path = self.entries_dir().join(&pkg.name);
            let contents = read_to_string(&path)
                .with_context(|| format!("failed to read `{}`", path.display()))?;
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
            let duration = SystemTime::now().duration_since(timestamp)?;
            Ok(duration.as_secs() < self.refresh_age * SECS_PER_DAY)
        })
    }

    fn repository_timestamp(&mut self, url: &str) -> Result<SystemTime> {
        let digest = url_digest(url);
        if !self.repository_timestamps.contains_key(&digest) {
            let path = self.repository_timestamps_dir().join(url_digest(url));
            let contents = read_to_string(&path)
                .with_context(|| format!("failed to read `{}`", path.display()))?;
            let secs = u64::from_str(&contents)?;
            let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
            self.repository_timestamps.insert(digest.clone(), timestamp);
        }
        #[allow(clippy::unwrap_used)]
        Ok(*self.repository_timestamps.get(&digest).unwrap())
    }

    pub fn fetch_versions(&mut self, name: &str) -> Result<Vec<Version>> {
        // smoelius: Ignore any errors that may occur while reading/deserializing.
        if let Ok(versions) = self.versions(name) {
            if self.versions_are_current(name).unwrap_or_default() {
                return Ok(versions);
            }
        }

        let crate_response = CRATES_IO_SYNC_CLIENT.get_crate(name)?;
        // smoelius: Avoid using anything other than `versions` from `CrateResponse`. In particular,
        // avoid using `crate_data`. The same data should be available in the crates.io index.
        let versions = crate_response.versions;
        self.write_versions(name, &versions)?;
        self.versions.insert(name.to_owned(), versions.clone());

        let timestamp = SystemTime::now();
        self.write_versions_timestamp(name, timestamp)?;
        self.versions_timestamps.insert(name.to_owned(), timestamp);

        Ok(versions)
    }

    fn versions(&mut self, name: &str) -> Result<Vec<Version>> {
        if !self.versions.contains_key(name) {
            let path = self.versions_dir().join(name);
            let contents = read_to_string(&path)
                .with_context(|| format!("failed to read `{}`", path.display()))?;
            let versions = serde_json::from_str::<Vec<Version>>(&contents)?;
            self.versions.insert(name.to_owned(), versions);
        }
        #[allow(clippy::unwrap_used)]
        Ok(self.versions.get(name).cloned().unwrap())
    }

    fn versions_are_current(&mut self, url: &str) -> Result<bool> {
        self.versions_timestamp(url).and_then(|timestamp| {
            let duration = SystemTime::now().duration_since(timestamp)?;
            Ok(duration.as_secs() < self.refresh_age * SECS_PER_DAY)
        })
    }

    fn versions_timestamp(&mut self, name: &str) -> Result<SystemTime> {
        if !self.versions_timestamps.contains_key(name) {
            let path = self.versions_timestamps_dir().join(name);
            let contents = read_to_string(&path)
                .with_context(|| format!("failed to read `{}`", path.display()))?;
            let secs = u64::from_str(&contents)?;
            let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
            self.versions_timestamps.insert(name.to_owned(), timestamp);
        }
        #[allow(clippy::unwrap_used)]
        Ok(*self.versions_timestamps.get(name).unwrap())
    }

    fn write_entry(&self, pkg_name: &str, entry: &Entry) -> Result<()> {
        create_dir_all(self.entries_dir()).with_context(|| "failed to create entries directory")?;
        let path = self.entries_dir().join(pkg_name);
        let json = serde_json::to_string_pretty(entry)?;
        write(&path, json).with_context(|| format!("failed to write `{}`", path.display()))?;
        Ok(())
    }

    fn write_repository_timestamp(&self, digest: &str, timestamp: SystemTime) -> Result<()> {
        create_dir_all(self.repository_timestamps_dir())
            .with_context(|| "failed to create repository timestamps directory")?;
        let path = self.repository_timestamps_dir().join(digest);
        let duration = timestamp.duration_since(SystemTime::UNIX_EPOCH)?;
        write(&path, duration.as_secs().to_string())
            .with_context(|| format!("failed to write `{}`", path.display()))?;
        Ok(())
    }

    fn write_versions(&self, name: &str, versions: &[Version]) -> Result<()> {
        create_dir_all(self.versions_dir())
            .with_context(|| "failed to create versions directory")?;
        let path = self.versions_dir().join(name);
        let json = serde_json::to_string_pretty(versions)?;
        write(&path, json).with_context(|| format!("failed to write `{}`", path.display()))?;
        Ok(())
    }

    fn write_versions_timestamp(&self, name: &str, timestamp: SystemTime) -> Result<()> {
        create_dir_all(self.versions_timestamps_dir())
            .with_context(|| "failed to create versions timestamps directory")?;
        let path = self.versions_timestamps_dir().join(name);
        let duration = timestamp.duration_since(SystemTime::UNIX_EPOCH)?;
        write(&path, duration.as_secs().to_string())
            .with_context(|| format!("failed to write `{}`", path.display()))?;
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

    fn base_dir(&self) -> &Path {
        let base_dir = self.tempdir.as_ref().map(TempDir::path);

        #[cfg(all(feature = "on-disk-cache", not(windows)))]
        return base_dir.unwrap_or(&CACHE_DIRECTORY);

        #[cfg(any(not(feature = "on-disk-cache"), windows))]
        #[allow(clippy::unwrap_used)]
        base_dir.unwrap()
    }
}

fn url_digest(url: &str) -> String {
    sha1_smol::Sha1::from(url).hexdigest()
}

fn repository_existence(repo_dir: &Path) -> Result<bool> {
    repo_dir.try_exists().with_context(|| {
        format!(
            "failed to determine whether `{}` exists",
            repo_dir.display()
        )
    })
}
