use anyhow::{ensure, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use rustsec::{advisory::Informational, Advisory, Database};
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    process::{Command, ExitStatus},
};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};
use tempfile::tempdir;

#[derive(Display, EnumIter, Eq, PartialEq)]
#[strum(serialize_all = "kebab_case")]
enum Outcome {
    Error,
    NotFound,
    Found,
}

#[allow(dead_code)]
struct Output {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

static RE: Lazy<Regex> =
    Lazy::new(|| Regex::new("(?m)^error: [0-9]+ denied warning[s]? found!$").unwrap());

fn main() -> Result<()> {
    let output = command_output(Command::new("cargo").arg("build").current_dir(".."))?;
    ensure!(output.status.success());

    let cargo_unmaintained = Path::new("../target/debug/cargo-unmaintained").canonicalize()?;

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

    println!("{} advisories for unmaintained packages", count);

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
            advisory_outcomes.push((advisory, Outcome::Error));
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

        let output = command_output(
            Command::new(&cargo_unmaintained)
                .args([
                    "unmaintained",
                    "--fail-fast",
                    "-p",
                    advisory.metadata.package.as_str(),
                ])
                .current_dir(&tempdir),
        )?;
        match output.status.code() {
            Some(0) => {
                println!("not found");
                advisory_outcomes.push((advisory, Outcome::NotFound));
            }
            Some(1) => {
                println!("found");
                advisory_outcomes.push((advisory, Outcome::Found));
            }
            Some(2) => {
                println!("error:\n```\n{}\n```", output.stderr.trim_end());
                advisory_outcomes.push((advisory, Outcome::Error));
            }
            _ => panic!("exit code should be <= 2"),
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

    for wanted in Outcome::iter() {
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
            )
        }
    }
}

fn advisory_url(advisory: &Advisory) -> String {
    format!("https://rustsec.org/advisories/{}.html", advisory.id())
}

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
