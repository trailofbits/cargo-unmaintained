use super::{RepoStatus, Url};
use anyhow::Result;
use regex::Regex;
use std::{sync::LazyLock, time::SystemTime};

#[allow(clippy::unwrap_used)]
static GITHUB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://github\.com/([^/?#]+)/([^/?#]+?)(?:\.git)?(?:[/#?]|$)").unwrap()
});

/// Extracts `(owner, repo)` from a Github URL.
/// Returns `None` if the URL does not match `https://github.com/{owner}/{repo}`.
pub(crate) fn parse_github_url(url: &str) -> Option<(&str, &str)> {
    let captures = GITHUB_RE.captures(url)?;
    #[allow(clippy::unwrap_used)]
    let owner = captures.get(1).unwrap().as_str();
    #[allow(clippy::unwrap_used)]
    let repo = captures.get(2).unwrap().as_str();
    Some((owner, repo))
}

/// A Github repository extracted from a package URL.
pub(crate) struct GithubRepo<'a> {
    pub url: Url<'a>,
    pub owner: &'a str,
    pub repo: &'a str,
}

impl<'a> GithubRepo<'a> {
    pub(crate) fn from_url(url: Url<'a>) -> Option<Self> {
        let (owner, repo) = parse_github_url(url.as_str())?;
        Some(Self {
            url: canonical_github_url(url),
            owner,
            repo,
        })
    }
}

pub(crate) fn canonical_github_url(url: Url<'_>) -> Url<'_> {
    let shortened = url.shorten().unwrap_or(url);
    shortened
        .as_str()
        .strip_suffix(".git")
        .map_or(shortened, Url::from)
}

pub(crate) trait Github {
    fn load_token(f: impl FnOnce(&str) -> Result<()>) -> Result<bool>;
    fn save_token() -> Result<()>;
    fn archival_status(url: Url) -> Result<RepoStatus<()>>;
    fn prefetch<'a>(repos: &'a [GithubRepo<'a>]) -> Result<Vec<RepoStatus<'a, SystemTime>>>;
}

// smoelius: If `__real_github` is enabled, we assume that `--all-features` was passed and therefore
// disable `__mock_github`.

#[cfg(all(feature = "__mock_github", not(feature = "__real_github")))]
mod mock;
#[cfg(all(feature = "__mock_github", not(feature = "__real_github")))]
pub use mock::Impl;

#[cfg(any(not(feature = "__mock_github"), feature = "__real_github"))]
mod real;
#[cfg(any(not(feature = "__mock_github"), feature = "__real_github"))]
pub use real::Impl;
#[cfg(any(not(feature = "__mock_github"), feature = "__real_github"))]
pub use real::util;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_url_accepts_common_forms() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo"),
            Some(("owner", "repo"))
        );
        assert_eq!(
            parse_github_url("https://github.com/owner/repo.git"),
            Some(("owner", "repo"))
        );
        assert_eq!(
            parse_github_url("https://github.com/owner/repo/"),
            Some(("owner", "repo"))
        );
    }

    #[test]
    fn parse_github_url_stops_at_query() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo?tab=readme"),
            Some(("owner", "repo"))
        );
    }

    #[test]
    fn parse_github_url_stops_at_fragment() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo#readme"),
            Some(("owner", "repo"))
        );
    }

    #[test]
    fn parse_github_url_rejects_incomplete_or_non_github_urls() {
        assert_eq!(parse_github_url("https://github.com/"), None);
        assert_eq!(parse_github_url("https://github.com/owner"), None);
        assert_eq!(parse_github_url("https://example.com/owner/repo"), None);
    }

    #[test]
    fn canonical_github_url_normalizes_aliases() {
        assert_eq!(
            canonical_github_url("https://github.com/owner/repo.git".into()).as_str(),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            canonical_github_url("https://github.com/owner/repo/tree/main".into()).as_str(),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            canonical_github_url("https://github.com/owner/repo.git/tree/main".into()).as_str(),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn github_repo_from_url_returns_parts_and_canonical_url() {
        let github_repo =
            GithubRepo::from_url("https://github.com/owner/repo.git/tree/main".into());
        assert!(github_repo.is_some());
        let Some(github_repo) = github_repo else {
            return;
        };

        assert_eq!(github_repo.url.as_str(), "https://github.com/owner/repo");
        assert_eq!(github_repo.owner, "owner");
        assert_eq!(github_repo.repo, "repo");
    }
}
