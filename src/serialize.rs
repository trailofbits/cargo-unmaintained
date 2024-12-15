use super::{OutdatedDep, RepoStatus, UnmaintainedPkg, SECS_PER_DAY};
use cargo_metadata::semver::{Version, VersionReq};
use serde::Serialize;

impl Serialize for UnmaintainedPkg<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerializableUnmaintainedPkg::new(self).serialize(serializer)
    }
}

#[derive(Serialize)]
struct SerializableUnmaintainedPkg<'pkg, 'dep> {
    name: &'pkg str,
    version: &'pkg Version,
    repo_status: SerializableRepoStatus,
    outdated_deps: Vec<SerializableOutdatedDep<'pkg, 'dep>>,
}

#[derive(Serialize)]
struct SerializableOutdatedDep<'pkg, 'dep> {
    name: &'pkg str,
    req: &'pkg VersionReq,
    version_used: &'pkg Version,
    version_latest: &'dep Version,
}

#[derive(Serialize)]
pub enum SerializableRepoStatus {
    Uncloneable,
    Unnamed,
    Age(u64),
    Unassociated,
    Nonexistent,
    Archived,
}

impl<'pkg, 'dep> SerializableUnmaintainedPkg<'pkg, 'dep> {
    fn new(value: &'dep UnmaintainedPkg<'pkg>) -> Self {
        let UnmaintainedPkg {
            pkg,
            repo_age,
            newer_version_is_available: _,
            outdated_deps,
        } = value;
        SerializableUnmaintainedPkg {
            name: &pkg.name,
            version: &pkg.version,
            repo_status: SerializableRepoStatus::from(*repo_age),
            outdated_deps: outdated_deps
                .iter()
                .map(SerializableOutdatedDep::new)
                .collect(),
        }
    }
}

impl<'pkg, 'dep> SerializableOutdatedDep<'pkg, 'dep> {
    fn new(value: &'dep OutdatedDep<'pkg>) -> Self {
        let OutdatedDep {
            dep,
            version_used,
            version_latest,
        } = value;
        SerializableOutdatedDep {
            name: &dep.name,
            req: &dep.req,
            version_used,
            version_latest,
        }
    }
}

impl From<RepoStatus<'_, u64>> for SerializableRepoStatus {
    fn from(value: RepoStatus<'_, u64>) -> Self {
        match value {
            RepoStatus::Uncloneable(_) => SerializableRepoStatus::Uncloneable,
            RepoStatus::Unnamed => SerializableRepoStatus::Unnamed,
            RepoStatus::Success(_, value) => SerializableRepoStatus::Age(value / SECS_PER_DAY),
            RepoStatus::Unassociated(_) => SerializableRepoStatus::Unassociated,
            RepoStatus::Nonexistent(_) => SerializableRepoStatus::Nonexistent,
            RepoStatus::Archived(_) => SerializableRepoStatus::Archived,
        }
    }
}
