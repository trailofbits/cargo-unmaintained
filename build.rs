use std::{env::var, fs::write, path::PathBuf};

const TOKEN_PATH: &str = if cfg!(not(windows)) {
    "$HOME/.config/cargo-unmaintained/token.txt"
} else {
    "%LOCALAPPDATA%\\\\cargo-unmaintained\\\\token.txt"
};

fn main() {
    println!("cargo:rustc-cfg=__warnings");

    #[cfg(all(feature = "cache-repositories", not(feature = "on-disk-cache")))]
    println!("cargo:warning=Feature `cache-repositories` has been renamed to `on-disk-cache`");

    let out_dir = var("OUT_DIR").unwrap();
    let path = PathBuf::from(out_dir).join("after_help.rs");
    let contents = format!(
        r#"const AFTER_HELP: &str = "\
The `GITHUB_TOKEN_PATH` environment variable can be set to the path of a file containing a \
personal access token. If set, cargo-unmaintained will use this token to authenticate to GitHub \
and check whether packages' repositories have been archived.

Alternatively, the `GITHUB_TOKEN` environment variable can be set to a personal access token. \
However, use of `GITHUB_TOKEN_PATH` is recommended as it is less likely to leak the token.

If neither `GITHUB_TOKEN_PATH` nor `GITHUB_TOKEN` is set, but a file exists at {TOKEN_PATH}, \
cargo-unmaintained will use that file's contents as a personal access token.

Unless --no-exit-code is passed, the exit status is 0 if no unmaintained packages were found and \
no irrecoverable errors occurred, 1 if unmaintained packages were found, and 2 if an irrecoverable \
error occurred.";
"#,
    );
    write(path, contents).unwrap();
}
