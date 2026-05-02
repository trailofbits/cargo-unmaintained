use super::GithubRepo;
use crate::{RepoStatus, Url, curl};
use anyhow::{Result, anyhow, bail};
use elaborate::std::io::ReadContext;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, SystemTime},
};

mod map_ext;
use map_ext::MapExt;

pub mod util;

const OK: u32 = 200;

// GitHub GraphQL overview and call structure:
// https://docs.github.com/graphql
// https://docs.github.com/graphql/guides/forming-calls-with-graphql
const GRAPHQL_URL: &str = "https://api.github.com/graphql";
const BATCH_SIZE: usize = 100;

thread_local! {
    static REPOSITORY_CACHE: RefCell<HashMap<String, Option<Rc<serde_json::Value>>>> = RefCell::new(HashMap::new());
}

pub struct Impl;

impl super::Github for Impl {
    fn load_token(f: impl FnOnce(&str) -> Result<()>) -> Result<bool> {
        util::load_token(f)
    }

    fn save_token() -> Result<()> {
        util::save_token()
    }

    fn archival_status(url: Url) -> Result<RepoStatus<()>> {
        let (url, owner, repo) = match_github_url(url)?;

        let Some(repository) = repository(owner, repo)? else {
            return Ok(RepoStatus::Nonexistent(url));
        };

        if repository
            .as_object()
            .and_then(|map| map.get_bool("archived"))
            .unwrap_or_default()
        {
            Ok(RepoStatus::Archived(url))
        } else {
            Ok(RepoStatus::Success(url, ()))
        }
    }

    fn prefetch<'a>(repos: &'a [GithubRepo<'a>]) -> Result<Vec<RepoStatus<'a, SystemTime>>> {
        if repos.is_empty() {
            return Ok(Vec::new());
        }

        let token = util::PERSONAL_TOKEN
            .get()
            .ok_or_else(|| anyhow!("no personal token available for GraphQL prefetch"))?;

        let mut repo_statuses = Vec::with_capacity(repos.len());

        for chunk in repos.chunks(BATCH_SIZE) {
            let query = build_graphql_query(chunk);
            let variables = build_graphql_variables(chunk);
            let response = send_graphql_query(token, &query, &variables)?;
            let chunk_statuses = parse_graphql_response(&response, chunk)?;
            repo_statuses.extend(chunk_statuses);
        }

        Ok(repo_statuses)
    }
}

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn repository(owner: &str, repo: &str) -> Result<Option<Rc<serde_json::Value>>> {
    REPOSITORY_CACHE.with_borrow_mut(|repository_cache| {
        let key = format!("{owner}/{repo}");

        if let Some(repo) = repository_cache.get(&key) {
            return Ok(repo.clone());
        }

        match repository_uncached(owner, repo) {
            Ok(repository) => Ok(repository_cache
                .entry(key)
                .or_insert(Some(Rc::new(repository)))
                .clone()),
            Err(error) => {
                repository_cache.insert(key, None);
                Err(error)
            }
        }
    })
}

fn repository_uncached(owner: &str, repo: &str) -> Result<serde_json::Value> {
    call_api(owner, repo, None, &[])
}

fn match_github_url(url: Url<'_>) -> Result<(Url<'_>, &str, &str)> {
    let Some(github_repo) = GithubRepo::from_url(url) else {
        bail!("failed to match GitHub url: {url}");
    };

    Ok((github_repo.url, github_repo.owner, github_repo.repo))
}

fn call_api(
    owner: &str,
    repo: &str,
    endpoint: Option<&str>,
    mut data: &[u8],
) -> Result<serde_json::Value> {
    let url_string = format!(
        "https://api.github.com/repos/{owner}/{repo}{}",
        endpoint
            .map(|endpoint| String::from("/") + endpoint)
            .unwrap_or_default(),
    );

    let mut list = ::curl::easy::List::new();
    list.append("User-Agent: cargo-unmaintained")?;
    if let Some(token) = util::PERSONAL_TOKEN.get() {
        list.append(&format!("Authorization: Bearer {token}"))?;
    }

    let mut handle = curl::handle(url_string.as_str().into())?;
    handle.http_headers(list)?;
    let mut response = Vec::new();
    {
        let mut transfer = handle.transfer();
        transfer.read_function(|buf| {
            #[allow(clippy::unwrap_used)]
            let len = data.read_wc(buf).unwrap();
            Ok(len)
        })?;
        transfer.write_function(|other| {
            response.extend_from_slice(other);
            Ok(other.len())
        })?;
        transfer.perform()?;
    }

    let response_code = handle.response_code()?;

    // smoelius: Should the next statement handle 404s, like `curl::existence` does?
    if response_code != OK {
        bail!("unexpected response code: {response_code}");
    }

    let value = serde_json::from_slice::<serde_json::Value>(&response)?;

    Ok(value)
}

fn build_graphql_query(repos: &[GithubRepo<'_>]) -> String {
    // Query.repository(owner:, name:) reference:
    // https://docs.github.com/graphql/reference/queries#repository
    //
    // Repository and Commit fields used below:
    // https://docs.github.com/graphql/reference/objects#repository
    // https://docs.github.com/graphql/reference/objects#commit
    let mut query = String::from("query(");
    for i in 0..repos.len() {
        if i > 0 {
            query.push_str(", ");
        }
        query.push_str(&format!("$owner{i}: String!, $name{i}: String!"));
    }
    query.push_str(") {");
    for i in 0..repos.len() {
        query.push_str(&format!(
            r" repo{i}: repository(owner: $owner{i}, name: $name{i}) {{
                isArchived
                pushedAt
                defaultBranchRef {{
                    target {{
                        ... on Commit {{
                            committedDate
                        }}
                    }}
                }}
            }}",
        ));
    }
    query.push_str(" }");
    query
}

fn build_graphql_variables(repos: &[GithubRepo<'_>]) -> serde_json::Map<String, serde_json::Value> {
    repos
        .iter()
        .enumerate()
        .flat_map(|(i, repo)| {
            [
                (format!("owner{i}"), serde_json::json!(repo.owner)),
                (format!("name{i}"), serde_json::json!(repo.repo)),
            ]
        })
        .collect()
}

fn send_graphql_query(
    token: &str,
    query: &str,
    variables: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value> {
    let body = serde_json::json!({ "query": query, "variables": variables });
    let body_bytes = serde_json::to_vec(&body)?;
    let mut data: &[u8] = &body_bytes;

    let mut list = ::curl::easy::List::new();
    list.append("User-Agent: cargo-unmaintained")?;
    list.append(&format!("Authorization: Bearer {token}"))?;
    list.append("Content-Type: application/json")?;

    let mut handle = curl::handle(GRAPHQL_URL.into())?;
    handle.post(true)?;
    let body_len = u64::try_from(body_bytes.len())?;
    handle.post_field_size(body_len)?;
    handle.http_headers(list)?;

    let mut response = Vec::new();
    {
        let mut transfer = handle.transfer();
        transfer.read_function(|buf| {
            #[allow(clippy::unwrap_used)]
            let len = data.read_wc(buf).unwrap();
            Ok(len)
        })?;
        transfer.write_function(|other| {
            response.extend_from_slice(other);
            Ok(other.len())
        })?;
        transfer.perform()?;
    }

    let response_code = handle.response_code()?;
    if response_code != OK {
        bail!("GraphQL request failed with status {response_code}");
    }

    serde_json::from_slice(&response).map_err(Into::into)
}

fn parse_graphql_response<'a>(
    response: &serde_json::Value,
    repos: &'a [GithubRepo<'a>],
) -> Result<Vec<RepoStatus<'a, SystemTime>>> {
    let not_found_aliases = not_found_aliases(response)?;

    let data = response
        .as_object()
        .and_then(|obj| obj.get("data"))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            let errors = response.as_object().and_then(|obj| obj.get("errors"));
            match errors {
                Some(errs) => anyhow!("GraphQL query failed: {errs}"),
                None => anyhow!("GraphQL response missing 'data' field"),
            }
        })?;

    let mut repo_statuses = Vec::with_capacity(repos.len());

    for (i, repo) in repos.iter().enumerate() {
        let alias = format!("repo{i}");
        let Some(repo_data) = data.get(&alias) else {
            bail!("GraphQL response missing `{alias}` field");
        };

        if repo_data.is_null() {
            if !not_found_aliases.contains(&alias) {
                bail!("GraphQL response returned null for `{alias}` without a NOT_FOUND error");
            }
            repo_statuses.push(RepoStatus::Nonexistent(repo.url));
            continue;
        }

        let Some(obj) = repo_data.as_object() else {
            bail!("GraphQL response field `{alias}` is not an object");
        };

        let is_archived = obj.get_bool("isArchived").unwrap_or(false);

        // Prefer committedDate (matches `git log -1 --pretty=format:%ct` on default branch)
        // with pushedAt as fallback for empty repos.
        let timestamp_str = obj
            .get("defaultBranchRef")
            .and_then(serde_json::Value::as_object)
            .and_then(|branch| branch.get("target"))
            .and_then(serde_json::Value::as_object)
            .and_then(|target| target.get_str("committedDate"))
            .or_else(|| obj.get_str("pushedAt"));

        if is_archived {
            repo_statuses.push(RepoStatus::Archived(repo.url));
            continue;
        }

        let Some(ts_str) = timestamp_str else {
            // No timestamp data available (empty repo with no commits and no pushes).
            repo_statuses.push(RepoStatus::Success(repo.url, SystemTime::UNIX_EPOCH));
            continue;
        };

        let timestamp = parse_timestamp(ts_str)?;

        repo_statuses.push(RepoStatus::Success(repo.url, timestamp));
    }

    Ok(repo_statuses)
}

fn not_found_aliases(response: &serde_json::Value) -> Result<HashSet<String>> {
    let Some(errors) = response
        .as_object()
        .and_then(|obj| obj.get("errors"))
        .and_then(serde_json::Value::as_array)
    else {
        return Ok(HashSet::new());
    };

    let mut aliases = HashSet::new();
    for error in errors {
        let error_type = error
            .as_object()
            .and_then(|obj| obj.get("type"))
            .and_then(serde_json::Value::as_str);
        let alias = error
            .as_object()
            .and_then(|obj| obj.get("path"))
            .and_then(serde_json::Value::as_array)
            .and_then(|path| path.first())
            .and_then(serde_json::Value::as_str);

        match (error_type, alias) {
            (Some("NOT_FOUND"), Some(alias)) => {
                aliases.insert(alias.to_owned());
            }
            _ => bail!("GraphQL query failed: {errors:?}"),
        }
    }

    Ok(aliases)
}

fn parse_timestamp(iso: &str) -> Result<SystemTime> {
    let date_time = chrono::DateTime::parse_from_rfc3339(iso)?;
    let secs = u64::try_from(date_time.timestamp())?;
    Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}
