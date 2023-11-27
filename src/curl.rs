use anyhow::Result;
use curl::easy::Easy;

pub(crate) fn existence(url: &str) -> Result<Option<bool>> {
    let mut handle = Easy::new();
    handle.url(url)?;
    handle.transfer().perform()?;
    let response_code = handle.response_code()?;
    match response_code {
        200 => Ok(Some(true)),
        404 => Ok(Some(false)),
        _ => Ok(None),
    }
}
