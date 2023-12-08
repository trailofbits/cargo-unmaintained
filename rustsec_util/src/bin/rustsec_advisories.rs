use anyhow::{ensure, Result};
use cargo_metadata::MetadataCommand;
use once_cell::sync::Lazy;
use regex::Regex;
use rustsec::{advisory::Informational, Advisory, Database};
use rustsec_util::{
    cargo_unmaintained, command_output, display_advisory_outcomes, test_package, Outcome,
};
use std::{io::Write, path::Path, process::Command};
use strum_macros::{Display, EnumIter};

#[derive(Display, EnumIter, Eq, PartialEq)]
#[strum(serialize_all = "kebab_case")]
enum Reason {
    Error,
    Leaf,
    RecentlyUpdated,
    Other,
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

        let tempdir = test_package(advisory.metadata.package.as_str())?;

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

    display_advisory_outcomes(
        &advisory_outcomes
            .into_iter()
            .map(|(advisory, outcome)| {
                let url = advisory_url(&advisory);
                (advisory.metadata.package, url, outcome)
            })
            .collect::<Vec<_>>(),
    );

    Ok(())
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
