use crate::{RepoStatus, Url};
use anyhow::Result;

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
}

fn enabled(key: &str) -> bool {
    std::env::var(key).is_ok_and(|value| value != "0")
}
