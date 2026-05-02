use super::GithubRepo;
use crate::{RepoStatus, Url};
use anyhow::Result;
use std::time::SystemTime;

pub struct Impl;

impl super::Github for Impl {
    fn load_token(_f: impl FnOnce(&str) -> Result<()>) -> Result<bool> {
        Ok(true)
    }

    fn save_token() -> Result<()> {
        unimplemented!()
    }

    fn archival_status(url: Url) -> Result<RepoStatus<()>> {
        let key = format!(
            "ARCHIVAL_STATUS_{}",
            url.as_str()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        );
        if enabled(&key) {
            Ok(RepoStatus::Archived(url))
        } else {
            #[cfg(any())]
            if std::env::var(&key).is_err() {
                use std::io::{Write, stderr};
                #[allow(clippy::explicit_write)]
                writeln!(
                    stderr(),
                    "environment variable`{key}` is unset; defaulting to unarchived"
                )
                .unwrap();
            }

            Ok(RepoStatus::Success(url, ()))
        }
    }

    fn prefetch<'a>(_repos: &'a [GithubRepo<'a>]) -> Result<Vec<RepoStatus<'a, SystemTime>>> {
        // Mock prefetch returns empty results. Existing per-package mock logic
        // (ARCHIVAL_STATUS_* env vars) handles archival status on cache miss.
        Ok(Vec::new())
    }
}

fn enabled(key: &str) -> bool {
    std::env::var(key).is_ok_and(|value| value != "0")
}
