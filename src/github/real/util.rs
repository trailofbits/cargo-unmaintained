use anyhow::{Result, anyhow};
use elaborate::std::{
    env::var_wc,
    fs::{OpenOptionsContext, create_dir_all_wc, read_to_string_wc},
    io::{StdinContext, WriteContext},
    path::PathContext,
};
use std::{
    fs::{File, OpenOptions},
    io::stdin,
    path::PathBuf,
    sync::{LazyLock, OnceLock},
};

#[allow(clippy::unwrap_used)]
static CONFIG_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    #[cfg(not(windows))]
    {
        let base_directories = xdg::BaseDirectories::new();
        base_directories
            .create_config_directory("cargo-unmaintained")
            .unwrap()
    }
    #[cfg(windows)]
    {
        let local_app_data = var_wc("LOCALAPPDATA").unwrap();
        PathBuf::from(local_app_data).join("cargo-unmaintained")
    }
});

static TOKEN_PATH: LazyLock<PathBuf> = LazyLock::new(|| CONFIG_DIRECTORY.join("token.txt"));

pub(super) static PERSONAL_TOKEN: OnceLock<String> = OnceLock::new();

pub fn load_token(f: impl FnOnce(&str) -> Result<()>) -> Result<bool> {
    let token_untrimmed = if let Ok(path) = var_wc("GITHUB_TOKEN_PATH") {
        read_to_string_wc(&path)?
    } else if let Ok(token) = var_wc("GITHUB_TOKEN") {
        // smoelius: Suppress warning if `CI` is set, i.e., if running on GitHub.
        if var_wc("CI").is_err() {
            #[cfg(__warnings)]
            crate::warn!(
                "found a token in `GITHUB_TOKEN`; consider using the more secure method of \
                 setting `GITHUB_TOKEN_PATH` to the path of a file containing the token",
            );
        }
        token
    } else if TOKEN_PATH.try_exists_wc()? {
        read_to_string_wc(&*TOKEN_PATH)?
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
    f(&token)?;
    Ok(true)
}

pub(crate) fn save_token() -> Result<()> {
    println!("Please paste a personal access token below. The token needs no scopes.");

    let mut buf = String::new();

    {
        let n = stdin().read_line_wc(&mut buf)?;
        assert_eq!(buf.len(), n);
    }

    create_dir_all_wc(&*CONFIG_DIRECTORY)?;

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open_wc(&*TOKEN_PATH)?;
    set_permissions(&file, 0o600)?;
    file.write_all_wc(buf.as_bytes())?;

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
#[allow(clippy::disallowed_methods)]
fn set_permissions(file: &File, mode: u32) -> CargoResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = file.metadata()?.permissions();
    perms.set_mode(mode);
    file.set_permissions(perms)?;
    Ok(())
}

#[cfg(not(unix))]
#[allow(unused, clippy::disallowed_methods, clippy::unnecessary_wraps)]
fn set_permissions(file: &File, mode: u32) -> CargoResult<()> {
    Ok(())
}
