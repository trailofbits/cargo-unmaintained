use once_cell::sync::Lazy;
use regex::Regex;

#[allow(clippy::unwrap_used)]
static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https://[^/]*/[^/]*/[^/]*").unwrap());

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Url<'a>(&'a str);

impl<'a> Url<'a> {
    pub(crate) fn as_str(&self) -> &'a str {
        self.0
    }

    pub(crate) fn leak(self) -> Url<'static> {
        Url(self.0.to_owned().leak())
    }

    #[allow(clippy::unwrap_used)]
    pub(crate) fn shorten(self) -> Option<Self> {
        RE.captures(self.0)
            .map(|captures| captures.get(0).unwrap().as_str().into())
    }

    pub(crate) fn trim_trailing_slash(self) -> Self {
        self.0.strip_suffix('/').map_or(self, Self::from)
    }
}

impl<'a> std::fmt::Display for Url<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl<'a> From<&'a str> for Url<'a> {
    fn from(value: &'a str) -> Self {
        Self(value)
    }
}

/// Returns up to two urls associated with `pkg`:
///
/// - the repository url stored in the [`cargo_metadata::Package`]
/// - a "shortened" url consisting of just the domain and two fragments
pub(crate) fn urls(pkg: &cargo_metadata::Package) -> impl IntoIterator<Item = Url> {
    let mut urls = Vec::new();

    if let Some(url_string) = &pkg.repository {
        // smoelius: Without the use of `trim_trailing_slash`, whether a timestamp was obtained via
        // the GitHub API or a shallow clone would be distinguishable.
        let url = Url::from(url_string.as_str()).trim_trailing_slash();

        urls.push(url);

        if let Some(shortened_url) = url.shorten() {
            urls.push(shortened_url);
        }
    }

    urls
}
