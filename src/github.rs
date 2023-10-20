use anyhow::{anyhow, bail, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::fs::read_to_string;
use tokio::runtime;

#[allow(clippy::unwrap_used)]
static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https://github\.com/([^/]*)/([^/]*)").unwrap());

#[allow(clippy::unwrap_used)]
static RT: Lazy<runtime::Runtime> = Lazy::new(|| {
    runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
});

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

pub(crate) fn timestamp(url: &str) -> Result<(&str, u64)> {
    let (url, owner, repo) = {
        #[allow(clippy::unwrap_used)]
        if let Some(captures) = RE.captures(url) {
            assert_eq!(3, captures.len());
            (
                captures.get(0).unwrap().as_str(),
                captures.get(1).unwrap().as_str(),
                captures.get(2).unwrap().as_str(),
            )
        } else {
            bail!("failed to match GitHub url: {url}");
        }
    };

    let repo = repo.strip_suffix(".git").unwrap_or(repo);

    #[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
    let datetime = RT.block_on(async {
        let octocrab = octocrab::instance();

        let repository = octocrab.repos(owner, repo).get().await?;

        let default_branch = repository
            .default_branch
            .ok_or_else(|| anyhow!("{url} repository has no default branch"))?;

        let page = octocrab
            .repos(owner, repo)
            .list_commits()
            .branch(default_branch)
            .per_page(1)
            .send()
            .await?;

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

        Result::<_>::Ok(date)
    })?;

    let timestamp = datetime.timestamp().try_into()?;

    Ok((url, timestamp))
}
