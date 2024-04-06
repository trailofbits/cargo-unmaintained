use super::{curl, RepoStatus, Url};
use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    cell::RefCell,
    collections::HashMap,
    io::Read,
    rc::Rc,
    time::{Duration, SystemTime},
};

mod map_ext;
use map_ext::MapExt;

mod util;
pub(crate) use util::load_token;
use util::PERSONAL_TOKEN;

#[allow(clippy::unwrap_used)]
static RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^https://github\.com/(([^/]*)/([^/]*))").unwrap());

thread_local! {
    static REPOSITORY_CACHE: RefCell<HashMap<String, Option<Rc<serde_json::Value>>>> = RefCell::new(HashMap::new());
}

pub(crate) fn archival_status(url: Url) -> Result<RepoStatus<()>> {
    let (url, owner_slash_repo, owner, repo) = match_github_url(url)?;

    let Some(repository) = repository(owner_slash_repo, owner, repo)? else {
        return Ok(RepoStatus::Nonexistent(url));
    };

    if repository
        .as_object()
        .and_then(|map| map.get_bool("archived"))
        .unwrap_or_default()
    {
        Ok(RepoStatus::Archived(url))
    } else {
        Ok(RepoStatus::Success(url, ()))
    }
}

pub(crate) fn timestamp(url: Url) -> Result<Option<(Url, SystemTime)>> {
    let (url, owner_slash_repo, owner, repo) = match_github_url(url)?;

    let Some(repository) = repository(owner_slash_repo, owner, repo)? else {
        return Ok(None);
    };

    let default_branch = repository
        .as_object()
        .and_then(|map| map.get_str("default_branch"))
        .ok_or_else(|| anyhow!("{url} repository has no default branch"))?;

    let page = {
        let json = serde_json::json!({
            "sha": default_branch,
            "per_page": 1,
        });

        call_api(owner, repo, Some("commits"), json.to_string().as_bytes())
    }?;

    let item = page
        .as_array()
        .and_then(|array| array.first())
        .ok_or_else(|| anyhow!("{url} page has no items"))?;
    let git_user_time = item
        .as_object()
        .and_then(|map| map.get_object("commit"))
        .and_then(|map| map.get("committer"))
        .ok_or_else(|| anyhow!("{url} item commit has no committer"))?;
    let date = git_user_time
        .as_object()
        .and_then(|map| map.get_str("date"))
        .ok_or_else(|| anyhow!("{url} committer has no date"))?;

    let date_time = date.parse::<DateTime<Utc>>()?;
    let secs = date_time.timestamp().try_into()?;
    let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);

    Ok(Some((url, timestamp)))
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
// smoelius: `owner_slash_repo` is a hack to avoid calling `to_owned` on `owner` and `repo` just to
// perform a cache lookup.
fn repository(
    owner_slash_repo: &str,
    owner: &str,
    repo: &str,
) -> Result<Option<Rc<serde_json::Value>>> {
    REPOSITORY_CACHE.with_borrow_mut(|repository_cache| {
        if let Some(repo) = repository_cache.get(owner_slash_repo) {
            return Ok(repo.clone());
        }

        match repository_uncached(owner, repo) {
            Ok(repository) => Ok(repository_cache
                .entry(owner_slash_repo.to_owned())
                .or_insert(Some(Rc::new(repository)))
                .clone()),
            Err(error) => {
                repository_cache.insert(owner_slash_repo.to_owned(), None);
                Err(error)
            }
        }
    })
}

fn repository_uncached(owner: &str, repo: &str) -> Result<serde_json::Value> {
    call_api(owner, repo, None, &[])
}

fn match_github_url(url: Url) -> Result<(Url, &str, &str, &str)> {
    let (url_string, owner_slash_repo, owner, repo) = {
        #[allow(clippy::unwrap_used)]
        if let Some(captures) = RE.captures(url.as_str()) {
            assert_eq!(4, captures.len());
            (
                captures.get(0).unwrap().as_str(),
                captures.get(1).unwrap().as_str(),
                captures.get(2).unwrap().as_str(),
                captures.get(3).unwrap().as_str(),
            )
        } else {
            bail!("failed to match GitHub url: {url}");
        }
    };

    let repo = repo.strip_suffix(".git").unwrap_or(repo);

    Ok((url_string.into(), owner_slash_repo, owner, repo))
}

fn call_api(
    owner: &str,
    repo: &str,
    endpoint: Option<&str>,
    mut data: &[u8],
) -> Result<serde_json::Value> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}{}",
        endpoint
            .map(|endpoint| String::from("/") + endpoint)
            .unwrap_or_default(),
    );

    let mut list = ::curl::easy::List::new();
    list.append("User-Agent: cargo-unmaintained")?;
    if let Some(token) = PERSONAL_TOKEN.get() {
        list.append(&format!("Authorization: Bearer {token}"))?;
    }

    let mut handle = curl::handle((&url).as_str().into())?;
    handle.http_headers(list)?;
    let mut response = Vec::new();
    {
        let mut transfer = handle.transfer();
        transfer.read_function(|buf| {
            #[allow(clippy::unwrap_used)]
            let len = data.read(buf).unwrap();
            Ok(len)
        })?;
        transfer.write_function(|other| {
            response.extend_from_slice(other);
            Ok(other.len())
        })?;
        transfer.perform()?;
    }

    let response_code = handle.response_code()?;

    // smoelius: Should the next statement handle 404s, like `curl::existence` does?
    if response_code != 200 {
        bail!("unexpected response code: {response_code}");
    }

    let value = serde_json::from_slice::<serde_json::Value>(&response)?;

    Ok(value)
}
