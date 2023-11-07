use anyhow::{ensure, Result};
use cargo_metadata::MetadataCommand;
use once_cell::sync::Lazy;
use regex::Regex;
use rustsec::{advisory::Informational, Advisory, Database};
use std::{
    env::consts::EXE_SUFFIX,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};
use tempfile::tempdir;

#[derive(Display, EnumIter, Eq, PartialEq)]
#[strum(serialize_all = "kebab_case")]
enum Reason {
    Error,
    Leaf,
    RecentlyUpdated,
    Other,
}

#[derive(Eq, PartialEq)]
enum Outcome {
    NotFound(Reason),
    Found,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NotFound(reason) => write!(f, "not found - {reason}"),
            Self::Found => write!(f, "found"),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct Output {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

static RE: Lazy<Regex> =
    Lazy::new(|| Regex::new("(?m)^error: [0-9]+ denied warning[s]? found!$").unwrap());

fn main() -> Result<()> {
    let mut advisory_outcomes = Vec::new();

    let mut advisories = {
        let database = Database::fetch()?;
        database
            .into_iter()
            .filter(|advisory| {
                advisory.metadata.informational == Some(Informational::Unmaintained)
                    && advisory.metadata.withdrawn.is_none()
                    && advisory.versions.unaffected().is_empty()
            })
            .collect::<Vec<_>>()
    };

    advisories.sort_by(|lhs, rhs| lhs.id().cmp(rhs.id()));

    let count = advisories.len();

    println!("{count} advisories for unmaintained packages");

    for advisory in advisories {
        print!("{}...", advisory.metadata.package);
        std::io::stdout().flush()?;

        let tempdir = tempdir()?;

        let output = command_output(
            Command::new("cargo")
                .args([
                    "init",
                    &format!("--name={}-test-package", advisory.metadata.package),
                ])
                .current_dir(&tempdir),
        )?;
        ensure!(output.status.success());

        let mut manifest = OpenOptions::new()
            .append(true)
            .open(tempdir.path().join("Cargo.toml"))?;
        writeln!(manifest, r#"{} = "*""#, advisory.metadata.package)?;

        let output = command_output(
            Command::new("cargo")
                .args(["generate-lockfile"])
                .current_dir(&tempdir),
        )?;
        if !output.status.success() {
            println!("error:\n```\n{}\n```", output.stderr.trim_end());
            advisory_outcomes.push((advisory, Outcome::NotFound(Reason::Error)));
            continue;
        }

        let output = command_output(
            Command::new("cargo")
                .args([
                    "audit",
                    "--color=never",
                    "--deny=unmaintained",
                    "--no-fetch",
                ])
                .current_dir(&tempdir),
        )?;
        ensure!(!output.status.success());
        ensure!(
            output.stderr.lines().any(|line| { RE.is_match(line) }),
            "{}",
            output.stderr
        );

        let output = command_output(&mut cargo_unmaintained(
            advisory.metadata.package.as_str(),
            tempdir.path(),
        ))?;
        if output.status.code() == Some(0) {
            if is_leaf(advisory.metadata.package.as_str(), tempdir.path())? {
                println!("leaf");
                advisory_outcomes.push((advisory, Outcome::NotFound(Reason::Leaf)));
                continue;
            }

            let mut command =
                cargo_unmaintained(advisory.metadata.package.as_str(), tempdir.path());
            let output = command_output(command.arg("--max-age=0"))?;
            if output.status.code() == Some(1) {
                println!("recently updated");
                advisory_outcomes.push((advisory, Outcome::NotFound(Reason::RecentlyUpdated)));
                continue;
            }
        }
        match output.status.code() {
            Some(0) => {
                println!("not found");
                advisory_outcomes.push((advisory, Outcome::NotFound(Reason::Other)));
            }
            Some(1) => {
                println!("found");
                advisory_outcomes.push((advisory, Outcome::Found));
            }
            Some(2) => {
                println!("error:\n```\n{}\n```", output.stderr.trim_end());
                advisory_outcomes.push((advisory, Outcome::NotFound(Reason::Error)));
            }
            _ => panic!("exit code should be <= 2: {output:#?}"),
        }
    }

    assert_eq!(count, advisory_outcomes.len());

    display_advisory_outcomes(&advisory_outcomes);

    Ok(())
}

fn display_advisory_outcomes(advisory_outcomes: &[(Advisory, Outcome)]) {
    let width_package = advisory_outcomes.iter().fold(0, |width, (advisory, _)| {
        std::cmp::max(width, advisory.metadata.package.as_str().len())
    });

    let width_url = advisory_outcomes.iter().fold(0, |width, (advisory, _)| {
        std::cmp::max(width, advisory_url(advisory).len())
    });

    for wanted in Reason::iter()
        .map(Outcome::NotFound)
        .chain(std::iter::once(Outcome::Found))
    {
        let advisories = advisory_outcomes
            .iter()
            .filter_map(|(advisory, actual)| {
                if *actual == wanted {
                    Some(advisory)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        println!("{wanted} ({})", advisories.len());

        for advisory in advisories {
            println!(
                "    {:width_package$}  {:width_url$}",
                advisory.metadata.package,
                advisory_url(advisory),
            );
        }
    }
}

fn advisory_url(advisory: &Advisory) -> String {
    format!("https://rustsec.org/advisories/{}.html", advisory.id())
}

fn is_leaf(name: &str, path: &Path) -> Result<bool> {
    let metadata = MetadataCommand::new().current_dir(path).exec()?;
    Ok(metadata.packages.iter().all(|pkg| {
        pkg.name == format!("{name}-test-package")
            || pkg.dependencies.iter().all(|dep| dep.path.is_some())
    }))
}

static CARGO_UNMAINTAINED: Lazy<PathBuf> = Lazy::new(|| {
    let output = command_output(Command::new("cargo").arg("build").current_dir("..")).unwrap();
    assert!(output.status.success());

    PathBuf::from(format!("../target/debug/cargo-unmaintained{EXE_SUFFIX}"))
        .canonicalize()
        .unwrap()
});

fn cargo_unmaintained(name: &str, dir: &Path) -> Command {
    let mut command = Command::new(&*CARGO_UNMAINTAINED);
    command
        .args(["unmaintained", "--fail-fast", "-p", name])
        .current_dir(dir);
    command
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn command_output(command: &mut Command) -> Result<Output> {
    let output = command.output()?;
    let status = output.status;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
