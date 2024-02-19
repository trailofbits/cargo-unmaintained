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

impl<'a> From<&'a String> for Url<'a> {
    fn from(value: &'a String) -> Self {
        Self(value)
    }
}
