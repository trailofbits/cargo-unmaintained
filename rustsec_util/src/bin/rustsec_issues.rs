use anyhow::{Context, Result};
use log::debug;
use once_cell::sync::Lazy;
use regex::Regex;
use rustsec_util::{
    cargo_unmaintained, command_output, display_advisory_outcomes, maybe_to_string, Outcome,
};
use std::{collections::HashSet, env::var, io::Write};

// smoelius: "../../../" is not ideal, but I am trying to avoid turning `cargo-unmaintained` into a
// multi-package project. For now, this seems like the best option.
#[path = "../../../src/github/util.rs"]
mod github_util;
use github_util::{load_token, RT};

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn main() -> Result<()> {
    if let Ok(path) = var("GITHUB_TOKEN_PATH") {
        load_token(&path)?;
    }

    let mut issues = Vec::new();
    for i in 1_u32.. {
        let page = RT.block_on(async {
            let octocrab = octocrab::instance();
            octocrab
                .issues("rustsec", "advisory-db")
                .list()
                .state(octocrab::params::State::All)
                .per_page(100)
                .page(i)
                .send()
                .await
        })?;
        if page.items.is_empty() {
            break;
        }
        issues.extend(page.items);
    }

    let mut issue_urls = issues
        .iter()
        .filter_map(|issue| {
            if !issue.title.contains("unmaintained")
                && !issue
                    .labels
                    .iter()
                    .any(|label| label.name == "Unmaintained")
            {
                return None;
            };
            let mut urls = issue.body.as_deref().map(extract_urls).unwrap_or_default();
            if urls.is_empty() {
                return None;
            }
            urls.sort_unstable();
            urls.dedup();
            Some((issue.number, urls))
        })
        .collect::<Vec<_>>();

    issue_urls.sort();

    let mut advisory_outcomes = Vec::new();

    for (number, urls) in issue_urls {
        let advisory_url = format!("https://github.com/rustsec/advisory-db/issues/{number}");
        let mut checked = HashSet::new();
        for url in urls {
            if let Some(name) = extract_package_name(url) {
                if checked.contains(name) {
                    continue;
                }
                checked.insert(name);
                print!("{name}...");
                std::io::stdout()
                    .flush()
                    .with_context(|| "failed to flush stdout")?;
                if is_unmaintained(name)? {
                    println!("found");
                    advisory_outcomes.push((name, advisory_url.clone(), Outcome::Found));
                } else {
                    println!("not found");
                    advisory_outcomes.push((
                        name,
                        advisory_url.clone(),
                        Outcome::NotFound(maybe_to_string::Unit::Unit),
                    ));
                }
            }
        }
    }

    display_advisory_outcomes(&advisory_outcomes);

    Ok(())
}

static URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\bhttps://[^\s()<>]*").unwrap());

fn extract_urls(body: &str) -> Vec<&str> {
    URL_RE.find_iter(body).map(|m| m.as_str()).collect()
}

static NAME_RES: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"^https://crates\.io/(crates/)?(?<name>[0-9A-Za-z_-]*)",
        r"^https://docs.rs/(?<name>[0-9A-Za-z_-]*)",
        r"^https://github\.com/[0-9A-Za-z_-]*/(?<name>[0-9A-Za-z_-]*)",
        r"^https://lib\.rs/crates/(?<name>[0-9A-Za-z_-]*)",
        r"^https://sourcegraph\.com/crates/(?<name>[0-9A-Za-z_-]*)",
    ]
    .into_iter()
    .map(|re| Regex::new(re).unwrap())
    .collect()
});

fn extract_package_name(url: &str) -> Option<&str> {
    if let Some(captures) = NAME_RES.iter().find_map(|re| re.captures(url)) {
        // smoelius: Don't print "ignoring" messages for explicitly ignored packages.
        let name = captures.name("name").unwrap().as_str();
        if ["advisory-db", "cargo", "rust"].contains(&name) {
            None
        } else {
            Some(name)
        }
    } else {
        println!("ignoring `{url}`");
        None
    }
}

fn is_unmaintained(name: &str) -> Result<bool> {
    let output = command_output(&mut cargo_unmaintained(name))?;

    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => {
            debug!("{output:#?}");
            Ok(false)
        }
    }
}
