use anyhow::{anyhow, Context, Result};
use std::{env::var, fs::read_to_string, sync::OnceLock};

pub(super) static PERSONAL_TOKEN: OnceLock<String> = OnceLock::new();

pub(crate) fn load_token(f: impl FnOnce(String) -> Result<()>) -> Result<bool> {
    let token_untrimmed = if let Ok(path) = var("GITHUB_TOKEN_PATH") {
        read_to_string(&path).with_context(|| format!("failed to read {path:?}"))?
    } else if let Ok(token) = var("GITHUB_TOKEN") {
        // smoelius: Suppress warning if `CI` is set, i.e., if running on GitHub.
        if var("CI").is_err() {
            #[cfg(feature = "__warnings")]
            crate::warn!(
                "found a token in `GITHUB_TOKEN`; consider using the more secure method of \
                 setting `GITHUB_TOKEN_PATH` to the path of a file containing the token",
            );
        }
        token
    } else {
        #[cfg(feature = "__warnings")]
        crate::warn!(
            "`GITHUB_TOKEN_PATH` and `GITHUB_TOKEN` are not set; archival statuses will not be \
             checked",
        );
        return Ok(false);
    };
    let token = token_untrimmed.trim_end().to_owned();
    PERSONAL_TOKEN
        .set(token.clone())
        .map_err(|_| anyhow!("`load_token` was already called"))?;
    f(token)?;
    Ok(true)
}
