use super::{RepoStatus, Url};
use anyhow::{Result, anyhow};
use curl::easy::{Easy, List};
use std::time::Duration;

const TIMEOUT: u64 = 60; // seconds

pub(crate) fn existence(url: Url) -> Result<RepoStatus<()>> {
    let mut handle = handle(url)?;
    let result = handle.transfer().perform();
    match result.and_then(|()| handle.response_code()) {
        Ok(200) => Ok(RepoStatus::Success(url, ())),
        Ok(404) => Ok(RepoStatus::Nonexistent(url)),
        Err(err) if err.is_operation_timedout() => Ok(RepoStatus::Nonexistent(url)),
        Ok(response_code) => Err(anyhow!("unexpected response code: {response_code}")),
        Err(err) => Err(err.into()),
    }
}

// smoelius: As of this writing, the Mercurial repository contains the following Python code:
//
//     try:
//         proto = pycompat.bytesurl(resp.getheader('content-type', ''))
//     except AttributeError:
//         proto = pycompat.bytesurl(resp.headers.get('content-type', ''))
//
//     ...
//
//     if not proto.startswith(b'application/mercurial-'):
//         ui.debug(b"requested URL: '%s'\n" % urlutil.hidepassword(requrl))
//         msg = _(
//             b"'%s' does not appear to be an hg repository:\n"
//             b"---%%<--- (%s)\n%s\n---%%<---\n"
//         ) % (safeurl, proto or b'no content-type', resp.read(1024))
//
// Thus, checking the content type seems to be as good a method as any for determining whether a url
// refers to a Mercurial repository.
//
// Reference: https://repo.mercurial-scm.org/hg/file/5cc8deb96b48/mercurial/httppeer.py#l342
pub(crate) fn is_mercurial_repo(url: Url) -> Result<bool> {
    let url_string = url.to_string() + "?cmd=capabilities";

    let mut list = List::new();
    list.append("Accept: application/mercurial-0.1")?;

    let mut handle = handle(url_string.as_str().into())?;
    handle.http_headers(list)?;
    handle.transfer().perform()?;

    let content_type = handle.content_type()?;

    Ok(content_type.is_some_and(|content_type| content_type.starts_with("application/mercurial-")))
}

pub(crate) fn handle(url: Url) -> Result<Easy> {
    let mut handle = Easy::new();
    handle.url(url.as_str())?;
    handle.follow_location(true)?;
    handle.timeout(Duration::from_secs(TIMEOUT))?;
    Ok(handle)
}
