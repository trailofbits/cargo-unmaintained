use anyhow::{Context, Result, ensure};
use cargo_metadata::MetadataCommand;
use cargo_unmaintained::{flush::Flush, packaging::temp_package};
use chrono::Utc;
use elaborate::std::process::ExitStatusContext;
use regex::Regex;
use rustsec::{Advisory, Database, advisory::Informational};
use std::{path::Path, process::Command, sync::LazyLock};
use strum_macros::{Display, EnumIter};

#[path = "rustsec_util/mod.rs"]
mod rustsec_util;
use rustsec_util::{Outcome, cargo_unmaintained, command_output, display_advisory_outcomes};

#[derive(Clone, Copy, Display, EnumIter, Eq, PartialEq)]
#[strum(serialize_all = "kebab_case")]
enum Reason {
    Error,
    Leaf,
    RecentlyUpdated,
    Other,
}

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?m)^error: [0-9]+ denied warning[s]? found!$").unwrap());

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    let mut advisory_outcomes = Vec::new();

    let mut advisories = {
        let database = Database::fetch()?;
        database
            .into_iter()
            .filter(|advisory| {
                // smoelius: `markdown` is an example of an advisory with patched versions:
                // https://rustsec.org/advisories/RUSTSEC-2022-0044.html
                // smoelius: `term` is an example of an advisory with unaffected versions:
                // https://rustsec.org/advisories/RUSTSEC-2018-0015.html
                advisory.metadata.informational == Some(Informational::Unmaintained)
                    && advisory.metadata.withdrawn.is_none()
                    && advisory.versions.patched().is_empty()
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
            println!("error:\n```\n{}\n```", output.stderr.trim_end().sanitize());
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

        // smoelius: When I run `curl` on a GitLab repo locally, I get a page that says "Verifying
        // that you are a human." For now, I am not giving those packages any special treatment in
        // this test.
        let output = command_output(&mut cargo_unmaintained(advisory.metadata.package.as_str()))?;
        if output.status.code_wc().is_ok_and(|code| code == 0) {
            if is_leaf(advisory.metadata.package.as_str(), tempdir.path())? {
                println!("leaf");
                advisory_outcomes.push((advisory, Outcome::NotFound(Reason::Leaf)));
                continue;
            }

            let mut command = cargo_unmaintained(advisory.metadata.package.as_str());
            let output = command_output(command.arg("--max-age=0"))?;
            if output.status.code_wc().is_ok_and(|code| code == 1) {
                println!("recently updated");
                advisory_outcomes.push((advisory, Outcome::NotFound(Reason::RecentlyUpdated)));
                continue;
            }
        }
        match output.status.code_wc() {
            Ok(0) => {
                println!("not found");
                advisory_outcomes.push((advisory, Outcome::NotFound(Reason::Other)));
            }
            Ok(1) => {
                println!("found");
                advisory_outcomes.push((advisory, Outcome::Found));
            }
            Ok(2) => {
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

const CUT_LINE: &str = "---";

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
    println!("{CUT_LINE}");
    println!(
        "As of {today}, the RustSec Advisory Database contains {count} active advisories for \
         unmaintained packages. Using the above conditions, `cargo-unmaintained` automatically \
         identifies {found} ({percentage}%) of them. These results can be reproduced by running \
         the [`rustsec_advisories`] example within this repository.",
    );
    println!("{CUT_LINE}");
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

trait Sanitize {
    fn sanitize(&self) -> String;
}

impl Sanitize for &str {
    fn sanitize(&self) -> String {
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("/(private|tmp)/[^)]*").unwrap());
        RE.replace_all(self, "[..]").to_string()
    }
}
