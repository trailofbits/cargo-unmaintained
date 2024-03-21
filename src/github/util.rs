use anyhow::{anyhow, Context, Result};
use std::{fs::read_to_string, sync::OnceLock};

pub(super) static PERSONAL_TOKEN: OnceLock<String> = OnceLock::new();

pub(crate) fn load_token(path: &str) -> Result<()> {
    let token = read_to_string(path).with_context(|| format!("failed to read {path:?}"))?;
    PERSONAL_TOKEN
        .set(token.trim_end().to_owned())
        .map_err(|_| anyhow!("`load_token` was already called"))?;
    Ok(())
}
