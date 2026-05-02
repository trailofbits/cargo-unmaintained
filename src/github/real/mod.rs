use super::GithubRepo;
use crate::{RepoStatus, Url, curl};
use anyhow::{Result, bail};
use elaborate::std::io::ReadContext;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::SystemTime};

mod map_ext;
use map_ext::MapExt;

pub mod util;

const OK: u32 = 200;

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
        let (url, owner, repo) = match_github_url(url)?;

        let Some(repository) = repository(owner, repo)? else {
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

    fn prefetch<'a>(_repos: &'a [GithubRepo<'a>]) -> Result<Vec<RepoStatus<'a, SystemTime>>> {
        Ok(Vec::new())
    }
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn repository(owner: &str, repo: &str) -> Result<Option<Rc<serde_json::Value>>> {
    REPOSITORY_CACHE.with_borrow_mut(|repository_cache| {
        let key = format!("{owner}/{repo}");

        if let Some(repo) = repository_cache.get(&key) {
            return Ok(repo.clone());
        }

        match repository_uncached(owner, repo) {
            Ok(repository) => Ok(repository_cache
                .entry(key)
                .or_insert(Some(Rc::new(repository)))
                .clone()),
            Err(error) => {
                repository_cache.insert(key, None);
                Err(error)
            }
        }
    })
}

fn repository_uncached(owner: &str, repo: &str) -> Result<serde_json::Value> {
    call_api(owner, repo, None, &[])
}

fn match_github_url(url: Url<'_>) -> Result<(Url<'_>, &str, &str)> {
    let Some(github_repo) = GithubRepo::from_url(url) else {
        bail!("failed to match GitHub url: {url}");
    };

    Ok((github_repo.url, github_repo.owner, github_repo.repo))
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
            let len = data.read_wc(buf).unwrap();
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
    if response_code != OK {
        bail!("unexpected response code: {response_code}");
    }

    let value = serde_json::from_slice::<serde_json::Value>(&response)?;

    Ok(value)
}
