use anyhow::{anyhow, bail, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    cell::RefCell,
    collections::HashMap,
    fs::read_to_string,
    rc::Rc,
    time::{Duration, SystemTime},
};
use tokio::runtime;

#[allow(clippy::unwrap_used)]
static RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^https://github\.com/(([^/]*)/([^/]*))").unwrap());

#[allow(clippy::unwrap_used)]
static RT: Lazy<runtime::Runtime> = Lazy::new(|| {
    runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
});

thread_local! {
    static REPOSITORY_CACHE: RefCell<HashMap<String, Option<Rc<octocrab::models::Repository>>>> = RefCell::new(HashMap::new());
}

pub(crate) fn load_token(path: &str) -> Result<()> {
    let token = read_to_string(path).with_context(|| format!("failed to read {path:?}"))?;
    RT.block_on(async {
        let octocrab = octocrab::Octocrab::builder()
            .personal_token(token.trim_end().to_owned())
            .build()?;
        let _octocrab = octocrab::initialise(octocrab);
        Ok(())
    })
}

pub(crate) fn archival_status(url: &str) -> Result<Option<(&str, bool)>> {
    let (url, owner_slash_repo, owner, repo) = match_github_url(url)?;

    let Some(repository) = repository(owner_slash_repo, owner, repo)? else {
        return Ok(None);
    };

    Ok(Some((url, repository.archived.unwrap_or_default())))
}

pub(crate) fn timestamp(url: &str) -> Result<Option<(&str, SystemTime)>> {
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
                Err(error)
            }
        }
    })
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn repository_uncached(owner: &str, repo: &str) -> Result<octocrab::models::Repository> {
    RT.block_on(async {
        let octocrab = octocrab::instance();

        octocrab.repos(owner, repo).get().await
    })
    .map_err(Into::into)
}

fn match_github_url(url: &str) -> Result<(&str, &str, &str, &str)> {
    let (url, owner_slash_repo, owner, repo) = {
        #[allow(clippy::unwrap_used)]
        if let Some(captures) = RE.captures(url) {
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

    Ok((url, owner_slash_repo, owner, repo))
}
