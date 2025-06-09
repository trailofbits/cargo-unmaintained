use crate::{RepoStatus, Url, curl};
use anyhow::{Result, bail};
use regex::Regex;
use std::{cell::RefCell, collections::HashMap, io::Read, rc::Rc, sync::LazyLock};

mod map_ext;
use map_ext::MapExt;

pub mod util;

#[allow(clippy::unwrap_used)]
static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^https://github\.com/(([^/]*)/([^/]*))").unwrap());

thread_local! {
    static REPOSITORY_CACHE: RefCell<HashMap<String, Option<Rc<serde_json::Value>>>> = RefCell::new(HashMap::new());
}

pub struct Impl;

impl super::Github for Impl {
    fn load_token(f: impl FnOnce(&str) -> Result<()>) -> Result<bool> {
        util::load_token(f)
    }

    fn save_token() -> Result<()> {
        util::save_token()
    }

    fn archival_status(url: Url) -> Result<RepoStatus<()>> {
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

fn match_github_url(url: Url<'_>) -> Result<(Url<'_>, &str, &str, &str)> {
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
    let url_string = format!(
        "https://api.github.com/repos/{owner}/{repo}{}",
        endpoint
            .map(|endpoint| String::from("/") + endpoint)
            .unwrap_or_default(),
    );

    let mut list = ::curl::easy::List::new();
    list.append("User-Agent: cargo-unmaintained")?;
    if let Some(token) = util::PERSONAL_TOKEN.get() {
        list.append(&format!("Authorization: Bearer {token}"))?;
    }

    let mut handle = curl::handle(url_string.as_str().into())?;
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
