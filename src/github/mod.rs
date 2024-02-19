use super::{RepoStatus, Url};
use anyhow::{anyhow, bail, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, SystemTime},
};

mod util;
pub(crate) use util::load_token;
use util::RT;

#[allow(clippy::unwrap_used)]
static RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^https://github\.com/(([^/]*)/([^/]*))").unwrap());

thread_local! {
    static REPOSITORY_CACHE: RefCell<HashMap<String, Option<Rc<octocrab::models::Repository>>>> = RefCell::new(HashMap::new());
}

pub(crate) fn archival_status(url: Url) -> Result<RepoStatus<()>> {
    let (url, owner_slash_repo, owner, repo) = match_github_url(url)?;

    let Some(repository) = repository(owner_slash_repo, owner, repo)? else {
        return Ok(RepoStatus::Nonexistent(url));
    };

    if repository.archived.unwrap_or_default() {
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
        .default_branch
        .as_ref()
        .ok_or_else(|| anyhow!("{url} repository has no default branch"))?;

    #[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
    let page = RT.block_on(async {
        let octocrab = octocrab::instance();

        octocrab
            .repos(owner, repo)
            .list_commits()
            .branch(default_branch)
            .per_page(1)
            .send()
            .await
    })?;

    let item = page
        .items
        .first()
        .ok_or_else(|| anyhow!("{url} page has no items"))?;
    let git_user_time = item
        .commit
        .committer
        .as_ref()
        .ok_or_else(|| anyhow!("{url} item commit has no committer"))?;
    let date = git_user_time
        .date
        .ok_or_else(|| anyhow!("{url} committer has no date"))?;

    let secs = date.timestamp().try_into()?;
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
) -> Result<Option<Rc<octocrab::models::Repository>>> {
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
                if let octocrab::Error::GitHub { source, .. } = &error {
                    if source.message == "Not Found" {
                        Ok(None)
                    } else {
                        Err(error.into())
                    }
                } else {
                    Err(error.into())
                }
            }
        }
    })
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn repository_uncached(owner: &str, repo: &str) -> octocrab::Result<octocrab::models::Repository> {
    RT.block_on(async {
        let octocrab = octocrab::instance();

        octocrab.repos(owner, repo).get().await
    })
}

fn match_github_url(url: Url) -> Result<(Url, &str, &str, &str)> {
    let (url_str, owner_slash_repo, owner, repo) = {
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

    Ok((url_str.into(), owner_slash_repo, owner, repo))
}
