use anyhow::Result;
use std::sync::LazyLock;
use tokio::runtime;

#[allow(clippy::unwrap_used)]
pub(super) static RT: LazyLock<runtime::Runtime> = LazyLock::new(|| {
    runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
});

pub(crate) fn load_token(token: &str) -> Result<()> {
    RT.block_on(async {
        let octocrab = octocrab::Octocrab::builder()
            .personal_token(token.trim_end())
            .build()?;
        let _octocrab = octocrab::initialise(octocrab);
        Ok(())
    })
}
