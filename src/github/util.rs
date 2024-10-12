use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use std::{
    env::var,
    fs::{create_dir_all, read_to_string, File, OpenOptions},
    io::{stdin, Write},
    path::PathBuf,
    sync::OnceLock,
};

#[allow(clippy::unwrap_used)]
static CONFIG_DIRECTORY: Lazy<PathBuf> = Lazy::new(|| {
    #[cfg(not(windows))]
    {
        let base_directories = xdg::BaseDirectories::new().unwrap();
        base_directories.get_config_file("cargo-unmaintained")
    }
    #[cfg(windows)]
    {
        let local_app_data = var("LOCALAPPDATA").unwrap();
        PathBuf::from(local_app_data).join("cargo-unmaintained")
    }
});

static TOKEN_PATH: Lazy<PathBuf> = Lazy::new(|| CONFIG_DIRECTORY.join("token.txt"));

pub(super) static PERSONAL_TOKEN: OnceLock<String> = OnceLock::new();

pub(crate) fn load_token(f: impl FnOnce(String) -> Result<()>) -> Result<bool> {
    let token_untrimmed = if let Ok(path) = var("GITHUB_TOKEN_PATH") {
        read_to_string(&path).with_context(|| format!("failed to read {path:?}"))?
    } else if let Ok(token) = var("GITHUB_TOKEN") {
        // smoelius: Suppress warning if `CI` is set, i.e., if running on GitHub.
        if var("CI").is_err() {
            #[cfg(__warnings)]
            crate::warn!(
                "found a token in `GITHUB_TOKEN`; consider using the more secure method of \
                 setting `GITHUB_TOKEN_PATH` to the path of a file containing the token",
            );
        }
        token
    } else if TOKEN_PATH.try_exists().with_context(|| {
        format!(
            "failed to determine whether `{}` exists",
            TOKEN_PATH.display()
        )
    })? {
        read_to_string(&*TOKEN_PATH).with_context(|| format!("failed to read {TOKEN_PATH:?}"))?
    } else {
        #[cfg(__warnings)]
        crate::warn!(
            "`GITHUB_TOKEN_PATH` and `GITHUB_TOKEN` are not set and no file was found at {}; \
             archival statuses will not be checked",
            TOKEN_PATH.display()
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

pub(crate) fn save_token() -> Result<()> {
    println!("Please paste a personal access token below. The token needs no scopes.");

    let mut buf = String::new();

    {
        let n = stdin()
            .read_line(&mut buf)
            .with_context(|| "failed to read stdin")?;
        assert_eq!(buf.len(), n);
    }

    create_dir_all(&*CONFIG_DIRECTORY).with_context(|| "failed to create config directory")?;

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&*TOKEN_PATH)
        .with_context(|| format!("failed to open `{}`", TOKEN_PATH.display()))?;
    set_permissions(&file, 0o600)?;
    file.write_all(buf.as_bytes())
        .with_context(|| format!("failed to write `{}`", TOKEN_PATH.display()))?;

    println!(
        "Personal access token written to `{}`",
        TOKEN_PATH.display()
    );

    Ok(())
}

type CargoResult<T> = Result<T>;

// smoelius: The below definitions of `set_permissions` were copied from:
// https://github.com/rust-lang/cargo/blob/1e6828485eea0f550ed7be46ef96107b46aeb162/src/cargo/util/config.rs#L1010-L1024
#[cfg(unix)]
#[cfg_attr(dylint_lib = "try_io_result", allow(try_io_result))]
fn set_permissions(file: &File, mode: u32) -> CargoResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = file.metadata()?.permissions();
    perms.set_mode(mode);
    file.set_permissions(perms)?;
    Ok(())
}

#[cfg(not(unix))]
#[allow(unused, clippy::unnecessary_wraps)]
fn set_permissions(file: &File, mode: u32) -> CargoResult<()> {
    Ok(())
}
