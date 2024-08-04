use super::{opts, Url, SECS_PER_DAY};
use anyhow::Result;
use termcolor::{Color, ColorSpec, WriteColor};

/// Repository statuses with the variants ordered by how "bad" they are.
///
/// A `RepoStatus` has a url only if it's not `Unnamed`. A `RepoStatus` has a value only if
/// it is `Success`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RepoStatus<'a, T> {
    Uncloneable(Url<'a>),
    Unnamed,
    Success(Url<'a>, T),
    Unassociated(Url<'a>),
    Nonexistent(Url<'a>),
    Archived(Url<'a>),
}

impl<'a, T> RepoStatus<'a, T> {
    pub fn as_success(&self) -> Option<(Url<'a>, &T)> {
        match self {
            Self::Uncloneable(_)
            | Self::Unnamed
            | Self::Unassociated(_)
            | Self::Nonexistent(_)
            | Self::Archived(_) => None,
            Self::Success(url, value) => Some((*url, value)),
        }
    }

    #[allow(dead_code)]
    pub fn is_success(&self) -> bool {
        self.as_success().is_some()
    }

    pub fn is_failure(&self) -> bool {
        self.as_success().is_none()
    }

    pub fn erase_url(self) -> RepoStatus<'static, T> {
        match self {
            Self::Uncloneable(_) => RepoStatus::Uncloneable(Url::default()),
            Self::Unnamed => RepoStatus::Unnamed,
            Self::Success(_, value) => RepoStatus::Success(Url::default(), value),
            Self::Unassociated(_) => RepoStatus::Unassociated(Url::default()),
            Self::Nonexistent(_) => RepoStatus::Nonexistent(Url::default()),
            Self::Archived(_) => RepoStatus::Archived(Url::default()),
        }
    }

    // smoelius: This isn't as bad as it looks. `leak_url` is used only when a `RepoStatus` needs to
    // be inserted into a global data structure. In such a case, the `RepoStatus`'s drop handler
    // would be called either never or when the program terminates. So the effect of leaking the url
    // is rather insignificant.
    pub fn leak_url(self) -> RepoStatus<'static, T> {
        match self {
            Self::Uncloneable(url) => RepoStatus::Uncloneable(url.leak()),
            Self::Unnamed => RepoStatus::Unnamed,
            Self::Success(url, value) => RepoStatus::Success(url.leak(), value),
            Self::Unassociated(url) => RepoStatus::Unassociated(url.leak()),
            Self::Nonexistent(url) => RepoStatus::Nonexistent(url.leak()),
            Self::Archived(url) => RepoStatus::Archived(url.leak()),
        }
    }

    pub fn map<U>(self, f: impl Fn(T) -> U) -> RepoStatus<'a, U> {
        match self {
            Self::Uncloneable(url) => RepoStatus::Uncloneable(url),
            Self::Unnamed => RepoStatus::Unnamed,
            Self::Success(url, value) => RepoStatus::Success(url, f(value)),
            Self::Unassociated(url) => RepoStatus::Unassociated(url),
            Self::Nonexistent(url) => RepoStatus::Nonexistent(url),
            Self::Archived(url) => RepoStatus::Archived(url),
        }
    }

    #[allow(clippy::panic)]
    pub fn map_failure<U>(self) -> RepoStatus<'a, U> {
        self.map(|_| panic!("unexpected `RepoStatus::Success`"))
    }
}

impl<'a, T, E> RepoStatus<'a, Result<T, E>> {
    pub fn transpose(self) -> Result<RepoStatus<'a, T>, E> {
        match self {
            Self::Uncloneable(url) => Ok(RepoStatus::Uncloneable(url)),
            Self::Unnamed => Ok(RepoStatus::Unnamed),
            Self::Success(url, Ok(value)) => Ok(RepoStatus::Success(url, value)),
            Self::Success(_, Err(error)) => Err(error),
            Self::Unassociated(url) => Ok(RepoStatus::Unassociated(url)),
            Self::Nonexistent(url) => Ok(RepoStatus::Nonexistent(url)),
            Self::Archived(url) => Ok(RepoStatus::Archived(url)),
        }
    }
}

/// Multiples of `max_age` that cause the color to go completely from yellow to red.
const SATURATION_MULTIPLIER: u64 = 3;

impl<'a> RepoStatus<'a, u64> {
    pub fn color(&self) -> Option<Color> {
        let age = match self {
            // smoelius: `Uncloneable` and `Unnamed` default to yellow.
            Self::Uncloneable(_) | Self::Unnamed => {
                return Some(Color::Rgb(u8::MAX, u8::MAX, 0));
            }
            Self::Success(_, age) => age,
            // smoelius: `Unassociated`, `Nonexistent`, and `Archived` default to red.
            Self::Unassociated(_) | Self::Nonexistent(_) | Self::Archived(_) => {
                return Some(Color::Rgb(u8::MAX, 0, 0));
            }
        };
        let age_in_days = age / SECS_PER_DAY;
        let Some(max_age_excess) = age_in_days.checked_sub(opts::get().max_age) else {
            // smoelius: `age_in_days` should be at least `max_age`. Otherwise, why are we here?
            debug_assert!(false);
            return None;
        };
        let subtrahend_u64 = if opts::get().max_age == 0 {
            u64::MAX
        } else {
            (max_age_excess * u64::from(u8::MAX)) / (SATURATION_MULTIPLIER * opts::get().max_age)
        };
        Some(Color::Rgb(
            u8::MAX,
            u8::MAX.saturating_sub(u8::try_from(subtrahend_u64).unwrap_or(u8::MAX)),
            0,
        ))
    }

    #[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
    #[cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]
    pub fn write(&self, stream: &mut impl WriteColor) -> std::io::Result<()> {
        match self {
            Self::Uncloneable(url) => {
                write_url(stream, *url)?;
                write!(stream, " is uncloneable")?;
                Ok(())
            }
            Self::Unnamed => write!(stream, "no repository"),
            Self::Success(url, age) => {
                write_url(stream, *url)?;
                write!(stream, " updated ")?;
                stream.set_color(ColorSpec::new().set_fg(self.color()))?;
                write!(stream, "{}", age / SECS_PER_DAY)?;
                stream.set_color(ColorSpec::new().set_fg(None))?;
                write!(stream, " days ago")?;
                Ok(())
            }
            Self::Unassociated(url) => {
                write!(stream, "not in ")?;
                write_url(stream, *url)?;
                Ok(())
            }
            Self::Nonexistent(url) => {
                write_url(stream, *url)?;
                write!(stream, " does not exist")?;
                Ok(())
            }
            Self::Archived(url) => {
                write_url(stream, *url)?;
                write!(stream, " archived")?;
                Ok(())
            }
        }
    }
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
#[cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]
fn write_url(stream: &mut impl WriteColor, url: Url) -> std::io::Result<()> {
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Blue)))?;
    write!(stream, "{url}")?;
    stream.set_color(ColorSpec::new().set_fg(None))?;
    Ok(())
}
