use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use std::fs::read_to_string;
use tokio::runtime;

#[allow(clippy::unwrap_used)]
pub(super) static RT: Lazy<runtime::Runtime> = Lazy::new(|| {
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
