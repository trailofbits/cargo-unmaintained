use super::RepoStatus;
use anyhow::{anyhow, Result};
use curl::easy::Easy;

pub(crate) fn existence(url: &str) -> Result<RepoStatus<()>> {
    let mut handle = Easy::new();
    handle.url(url)?;
    handle.transfer().perform()?;
    let response_code = handle.response_code()?;
    match response_code {
        200 => Ok(RepoStatus::Success(url, ())),
        404 => Ok(RepoStatus::Nonexistent(url)),
        _ => Err(anyhow!("unexpected response code: {response_code}")),
    }
}
