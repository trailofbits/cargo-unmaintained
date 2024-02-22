use super::{RepoStatus, Url};
use anyhow::{anyhow, Result};
use curl::easy::Easy;
use std::time::Duration;

const TIMEOUT: u64 = 60; // seconds

pub(crate) fn existence(url: Url) -> Result<RepoStatus<()>> {
    let mut handle = Easy::new();
    handle.url(url.as_str())?;
    handle.follow_location(true)?;
    handle.timeout(Duration::from_secs(TIMEOUT))?;
    let result = handle.transfer().perform();
    match result.and_then(|()| handle.response_code()) {
        Ok(200) => Ok(RepoStatus::Success(url, ())),
        Ok(404) => Ok(RepoStatus::Nonexistent(url)),
        Err(err) if err.is_operation_timedout() => Ok(RepoStatus::Nonexistent(url)),
        Ok(response_code) => Err(anyhow!("unexpected response code: {response_code}")),
        Err(err) => Err(err.into()),
    }
}
