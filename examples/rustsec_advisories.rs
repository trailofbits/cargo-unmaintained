use anyhow::{ensure, Context, Result};
use cargo_metadata::MetadataCommand;
use cargo_unmaintained::{flush::Flush, packaging::temp_package};
use chrono::Utc;
use once_cell::sync::Lazy;
use regex::Regex;
use rustsec::{advisory::Informational, Advisory, Database};
use std::{path::Path, process::Command};
use strum_macros::{Display, EnumIter};

#[path = "rustsec_util/mod.rs"]
mod rustsec_util;
use rustsec_util::{cargo_unmaintained, command_output, display_advisory_outcomes, Outcome};

#[derive(Clone, Copy, Display, EnumIter, Eq, PartialEq)]
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
        <_ as Flush>::flush(&mut std::io::stdout()).with_context(|| "failed to flush stdout")?;

        let tempdir = temp_package(advisory.metadata.package.as_str())?;

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

        let output = command_output(&mut cargo_unmaintained(advisory.metadata.package.as_str()))?;
        if output.status.code() == Some(0) {
            if is_leaf(advisory.metadata.package.as_str(), tempdir.path())? {
                println!("leaf");
                advisory_outcomes.push((advisory, Outcome::NotFound(Reason::Leaf)));
                continue;
            }

            let mut command = cargo_unmaintained(advisory.metadata.package.as_str());
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

    #[cfg_attr(dylint_lib = "supplementary", allow(suboptimal_pattern))]
    display_advisory_outcomes(
        &advisory_outcomes
            .iter()
            .map(|(advisory, outcome)| {
                let url = advisory_url(advisory);
                (&advisory.metadata.package, url, *outcome)
            })
            .collect::<Vec<_>>(),
    );

    println!("---");

    display_expected_readme_contents(
        &advisory_outcomes
            .iter()
            .map(|&(_, outcome)| outcome)
            .collect::<Vec<_>>(),
    );

    Ok(())
}

macro_rules! count {
    ($outcomes:expr, $pat:pat) => {
        $outcomes
            .iter()
            .filter(|outcome| matches!(outcome, $pat))
            .count()
    };
}

fn display_expected_readme_contents(outcomes: &[Outcome<Reason>]) {
    let today = Utc::now().date_naive();
    let count = outcomes.len();
    let found = count!(outcomes, Outcome::Found);
    let not_found = count!(outcomes, Outcome::NotFound(_));
    let error = count!(outcomes, Outcome::NotFound(Reason::Error));
    let leaf = count!(outcomes, Outcome::NotFound(Reason::Leaf));
    let recently_updated = count!(outcomes, Outcome::NotFound(Reason::RecentlyUpdated));
    let other = count!(outcomes, Outcome::NotFound(Reason::Other));
    #[cfg_attr(dylint_lib = "supplementary", allow(unnamed_constant))]
    let percentage = found * 100 / count;
    println!(
        "As of {today}, the RustSec Advisory Database contains {count} active advisories for \
         unmaintained packages. Using the above conditions, `cargo-unmaintained` automatically \
         identifies {found} ({percentage}) of them. These results can be reproduced by running \
         the [`rustsec_advisories`] binary within this repository.",
    );
    println!(
        "- Of the {not_found} packages in the RustSec Advisory Database _not_ identified by \
         `cargo-unmaintained`:"
    );
    println!("  - {error} do not build");
    println!("  - {leaf} are existent, unarchived leaves");
    println!("  - {recently_updated} were updated within the past 365 days");
    println!("  - {other} were not identified for other reasons",);
}

fn advisory_url(advisory: &Advisory) -> String {
    format!("https://rustsec.org/advisories/{}.html", advisory.id())
}

fn is_leaf(name: &str, path: &Path) -> Result<bool> {
    let metadata = MetadataCommand::new().current_dir(path).exec()?;
    Ok(metadata.packages.iter().all(|pkg| {
        pkg.name == format!("{name}-temp-package")
            || pkg.dependencies.iter().all(|dep| dep.path.is_some())
    }))
}
