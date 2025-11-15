use anyhow::{Context, Result};
use elaborate::std::{env::var_wc, path::PathContext, process::CommandContext};
use std::{
    env::consts::EXE_SUFFIX,
    path::PathBuf,
    process::{Command, ExitStatus},
    sync::LazyLock,
};

pub mod maybe_to_string;
use maybe_to_string::MaybeToString;

#[allow(dead_code)]
#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Outcome<T> {
    NotFound(T),
    Found,
}

impl<T: MaybeToString> std::fmt::Display for Outcome<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NotFound(reason) => write!(
                f,
                "not found{}",
                reason
                    .maybe_to_string()
                    .map(|s| format!(" - {s}"))
                    .unwrap_or_default()
            ),
            Self::Found => write!(f, "found"),
        }
    }
}

pub fn display_advisory_outcomes<T: MaybeToString + PartialEq + strum::IntoEnumIterator>(
    package_url_outcomes: &[(impl AsRef<str>, impl AsRef<str>, Outcome<T>)],
) {
    let width_package = package_url_outcomes
        .iter()
        .fold(0, |width, (package, _, _)| {
            std::cmp::max(width, package.as_ref().len())
        });

    let width_url = package_url_outcomes.iter().fold(0, |width, (_, url, _)| {
        std::cmp::max(width, url.as_ref().len())
    });

    for wanted in T::iter()
        .map(Outcome::NotFound)
        .chain(std::iter::once(Outcome::Found))
    {
        let package_urls = package_url_outcomes
            .iter()
            .filter_map(|(package, url, actual)| {
                if *actual == wanted {
                    Some((package, url))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        println!("{wanted} ({})", package_urls.len());

        for (package, url) in package_urls {
            println!(
                "    {:width_package$}  {:width_url$}",
                package.as_ref(),
                url.as_ref()
            );
        }
    }
}

static CARGO_UNMAINTAINED: LazyLock<PathBuf> = LazyLock::new(|| {
    let output = command_output(Command::new("cargo").arg("build")).unwrap();
    assert!(output.status.success());

    PathBuf::from(format!("target/debug/cargo-unmaintained{EXE_SUFFIX}"))
        .canonicalize_wc()
        .unwrap()
});

#[must_use]
pub fn cargo_unmaintained(name: &str) -> Command {
    let mut command = Command::new(&*CARGO_UNMAINTAINED);
    command.args(["unmaintained", "--fail-fast", "-p", name]);
    command
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
pub fn command_output(command: &mut Command) -> Result<Output> {
    let output = command
        .output_wc()
        .with_context(|| format!("failed to execute command: {command:?}"))?;
    let status = output.status;
    let stdout = String::from_utf8(output.stdout)?;
    if enabled("VERBOSE") {
        eprintln!("stdout=```\n{stdout}```");
    }
    let stderr = String::from_utf8(output.stderr)?;
    if enabled("VERBOSE") {
        eprintln!("stderr=```\n{stderr}```");
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn enabled(key: &str) -> bool {
    var_wc(key).is_ok_and(|value| value != "0")
}
