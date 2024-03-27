//! On-disk repositories cache
//!
//! The on-disk cache consists of two subdirectories:
//! - entries: Package data. Each file's name is that of package whose data it stores.
//! - repositories: Cloned repositories. Each subdirectory's name is the hash of the url that was
//!   cloned.
//!
//! An package's entry is considered current if all of the following conditions are met:
//! - A url associated with the package was successfully cloned.
//! - The clone was performed no more that `refresh_age` days ago.
//!
//! If either of the above conditions are not met, an attempt is made to refresh the entry.

use super::{urls, SECS_PER_DAY};
use anyhow::{anyhow, ensure, Context, Result};
use cargo_metadata::Package;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{create_dir_all, read_to_string, write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    time::{Duration, SystemTime},
};
use tempfile::{tempdir, TempDir};

#[derive(Clone, Deserialize, Serialize)]
struct Entry {
    named_url: String,
    cloned_url: String,
}

pub(crate) struct Cache {
    tempdir: Option<TempDir>,
    refresh_age: u64, // days
    entries: HashMap<String, Entry>,
    timestamps: HashMap<String, SystemTime>,
}

static CACHE_DIRECTORY: Lazy<PathBuf> = Lazy::new(|| {
    let base_directories = xdg::BaseDirectories::new().unwrap();
    base_directories
        .create_cache_directory("cargo-unmaintained")
        .unwrap()
});

impl Cache {
    pub fn new(temporary: bool, refresh_age: u64) -> Result<Self> {
        let tempdir = if temporary {
            tempdir().map(Option::Some)?
        } else {
            None
        };
        Ok(Self {
            tempdir,
            refresh_age,
            entries: HashMap::new(),
            timestamps: HashMap::new(),
        })
    }

    pub fn finalize(&mut self) {
        if let Some(tempdir) = self.tempdir.take() {
            drop(tempdir);
        } else {
            self.write_entries().unwrap();
            self.write_timestamps().unwrap();
        }
    }

    pub fn clone_repository<'a, 'b>(&'a mut self, pkg: &'b Package) -> Result<(String, PathBuf)> {
        // smoelius: Ignore any errors that may occur during reading/deserialization.
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
        self.entries.insert(
            pkg.name.clone(),
            Entry {
                named_url: pkg.repository.clone().unwrap(),
                cloned_url: url_and_dir.0.as_str().to_owned(),
            },
        );
        self.timestamps
            .insert(url_digest(&url_and_dir.0), SystemTime::now());
        Ok(url_and_dir)
    }

    fn clone_repository_uncached<'a, 'b>(&'a self, pkg: &'b Package) -> Result<(String, PathBuf)> {
        let mut errors = Vec::new();
        for url in urls(pkg) {
            let repo_dir = self.repositories_dir().join(url_digest(url.as_str()));
            let mut command = if repo_dir.try_exists()? {
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
        Err(anyhow!("{:#?}", errors))
    }

    fn entry(&mut self, pkg: &Package) -> Result<Entry> {
        if !self.entries.contains_key(&pkg.name) {
            let path = self.entries_dir().join(&pkg.name);
            let contents = read_to_string(path)?;
            let entry = serde_json::from_str::<Entry>(&contents)?;
            ensure!(
                pkg.repository.as_ref() == Some(&entry.named_url),
                "`pkg.repository` and `entry.named_url` differ"
            );
            self.entries.insert(pkg.name.clone(), entry);
        }
        Ok(self.entries.get(&pkg.name).cloned().unwrap())
    }

    fn repository_is_current(&mut self, url: &str) -> Result<bool> {
        self.timestamp(url).and_then(|timestamp| {
            let duration = SystemTime::now().duration_since(timestamp)?;
            Ok(duration.as_secs() < self.refresh_age * SECS_PER_DAY)
        })
    }

    fn timestamp(&mut self, url: &str) -> Result<SystemTime> {
        let digest = url_digest(url);
        if !self.timestamps.contains_key(&digest) {
            let path = self.timestamps_dir().join(url_digest(url));
            let contents = read_to_string(path)?;
            let secs = u64::from_str(&contents)?;
            let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
            self.timestamps.insert(digest.clone(), timestamp);
        }
        Ok(*self.timestamps.get(&digest).unwrap())
    }

    fn write_entries(&self) -> Result<()> {
        create_dir_all(self.entries_dir())?;
        for (pkg_name, entry) in self.entries.iter() {
            let path = self.entries_dir().join(pkg_name);
            let json = serde_json::to_string_pretty(entry)?;
            write(path, json)?;
        }
        Ok(())
    }

    fn write_timestamps(&self) -> Result<()> {
        create_dir_all(self.timestamps_dir())?;
        for (digest, timestamp) in self.timestamps.iter() {
            let path = self.timestamps_dir().join(digest);
            let duration = timestamp.duration_since(SystemTime::UNIX_EPOCH)?;
            write(&path, duration.as_secs().to_string())
                .with_context(|| format!("failed to write `{}`", path.display()))?;
        }
        Ok(())
    }

    fn entries_dir(&self) -> PathBuf {
        self.base_dir().join("entries")
    }

    fn repositories_dir(&self) -> PathBuf {
        self.base_dir().join("repositories")
    }

    fn timestamps_dir(&self) -> PathBuf {
        self.base_dir().join("timestamps")
    }

    fn base_dir(&self) -> &Path {
        self.tempdir
            .as_ref()
            .map_or(&CACHE_DIRECTORY, TempDir::path)
    }
}

fn url_digest(url: &str) -> String {
    sha1_smol::Sha1::from(url).hexdigest()
}
