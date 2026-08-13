//! Strict bounded loading of repository-owned discovery configuration.

use crate::path::{RepositoryPathObservation, observe_repository_path, open_regular_no_follow};
use a3_application::{
    ProjectIgnoreSettings, ProjectIgnoreSettingsFuture, ProjectIgnoreSettingsSource,
    ProjectIgnoreSettingsSourceFailure,
};
use a3_domain::{DiscoveryPolicy, ProjectIdentity, RepositoryPath};
use gix::bstr::ByteSlice;
use gix::ignore::glob::pattern::Case;
use serde::Deserialize;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::Path;

const PROJECT_CONFIG_PATH: &[u8] = b".a3/project.toml";

#[derive(Debug, Default)]
pub(crate) struct ProjectIgnore {
    search: gix::ignore::Search,
    configuration_present: bool,
    patterns: Vec<String>,
}

impl ProjectIgnore {
    pub(crate) fn matches(&self, path: &[u8], is_dir: bool, case: Case) -> bool {
        self.search
            .pattern_matching_relative_path(path.as_bstr(), Some(is_dir), case)
            .is_some()
    }

    fn into_settings(self) -> Result<ProjectIgnoreSettings, ProjectIgnoreSettingsSourceFailure> {
        ProjectIgnoreSettings::try_new(self.configuration_present, self.patterns)
            .map_err(|_| ProjectIgnoreSettingsSourceFailure::InvalidConfiguration)
    }
}

/// Reads only the dedicated exclusion-only `.a3/project.toml` configuration.
#[derive(Debug, Clone, Copy, Default)]
pub struct RepositoryProjectIgnoreSettingsSource;

impl ProjectIgnoreSettingsSource for RepositoryProjectIgnoreSettingsSource {
    fn read<'a>(&'a self, project: &'a ProjectIdentity) -> ProjectIgnoreSettingsFuture<'a> {
        Box::pin(async move {
            load_project_ignore(project.worktree().root().as_path(), DiscoveryPolicy::v1())
                .map_err(|error| match error {
                    ProjectConfigurationError::Invalid => {
                        ProjectIgnoreSettingsSourceFailure::InvalidConfiguration
                    }
                    ProjectConfigurationError::Io => {
                        ProjectIgnoreSettingsSourceFailure::Unavailable
                    }
                })?
                .into_settings()
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectConfiguration {
    discovery: DiscoveryConfiguration,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DiscoveryConfiguration {
    ignore: Vec<String>,
}

#[derive(Debug)]
pub(crate) enum ProjectConfigurationError {
    Invalid,
    Io,
}

pub(crate) fn load_project_ignore(
    root: &Path,
    policy: DiscoveryPolicy,
) -> Result<ProjectIgnore, ProjectConfigurationError> {
    let repository_path = RepositoryPath::try_from_bytes(PROJECT_CONFIG_PATH.to_vec())
        .map_err(|_| ProjectConfigurationError::Invalid)?;
    let (path, metadata) = match observe_repository_path(root, &repository_path)
        .map_err(|_| ProjectConfigurationError::Io)?
    {
        RepositoryPathObservation::Missing => return Ok(ProjectIgnore::default()),
        RepositoryPathObservation::SymbolicLink => {
            return Err(ProjectConfigurationError::Invalid);
        }
        RepositoryPathObservation::Present { path, metadata } => (path, metadata),
    };
    if !metadata.is_file() || metadata.len() > policy.max_config_bytes() as u64 {
        return Err(ProjectConfigurationError::Invalid);
    }

    let file = open_regular_no_follow(&path).map_err(|_| ProjectConfigurationError::Io)?;
    let maximum_read = u64::try_from(policy.max_config_bytes())
        .map_err(|_| ProjectConfigurationError::Invalid)?
        .saturating_add(1);
    let mut bytes = Vec::with_capacity(policy.max_config_bytes().min(metadata.len() as usize));
    file.take(maximum_read)
        .read_to_end(&mut bytes)
        .map_err(|_| ProjectConfigurationError::Io)?;
    if bytes.len() > policy.max_config_bytes() {
        return Err(ProjectConfigurationError::Invalid);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| ProjectConfigurationError::Invalid)?;
    let parsed: ProjectConfiguration =
        toml::from_str(text).map_err(|_| ProjectConfigurationError::Invalid)?;
    validate_patterns(&parsed.discovery.ignore, policy)?;

    let expected_pattern_count = parsed.discovery.ignore.len();
    let settings_patterns = parsed.discovery.ignore.clone();
    let patterns = parsed
        .discovery
        .ignore
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    let search = gix::ignore::Search::from_overrides(patterns, Default::default());
    let parsed_pattern_count = search
        .patterns
        .iter()
        .map(|list| list.patterns.len())
        .sum::<usize>();
    if parsed_pattern_count != expected_pattern_count {
        return Err(ProjectConfigurationError::Invalid);
    }
    Ok(ProjectIgnore {
        search,
        configuration_present: true,
        patterns: settings_patterns,
    })
}

fn validate_patterns(
    patterns: &[String],
    policy: DiscoveryPolicy,
) -> Result<(), ProjectConfigurationError> {
    if patterns.len() > policy.max_ignore_patterns() {
        return Err(ProjectConfigurationError::Invalid);
    }
    for pattern in patterns {
        let bytes = pattern.as_bytes();
        if bytes.is_empty()
            || bytes.len() > policy.max_ignore_pattern_bytes()
            || bytes.contains(&0)
            || bytes.contains(&b'\n')
            || bytes.contains(&b'\r')
            || matches!(bytes.first(), Some(b'!') | Some(b'#'))
            || bytes.split(|byte| *byte == b'/').any(|part| part == b"..")
            || gix::ignore::parse(bytes, false).next().is_none()
        {
            return Err(ProjectConfigurationError::Invalid);
        }
    }
    Ok(())
}

impl From<io::Error> for ProjectConfigurationError {
    fn from(_value: io::Error) -> Self {
        Self::Io
    }
}

#[cfg(test)]
mod tests {
    use super::validate_patterns;
    use a3_domain::DiscoveryPolicy;

    #[test]
    fn project_patterns_are_exclusion_only_and_bounded() {
        let policy = DiscoveryPolicy::v1();
        assert!(validate_patterns(&["generated/**".to_owned()], policy).is_ok());
        assert!(validate_patterns(&["!secret.txt".to_owned()], policy).is_err());
        assert!(validate_patterns(&["../outside".to_owned()], policy).is_err());
        assert!(
            validate_patterns(
                &["x".repeat(policy.max_ignore_pattern_bytes().saturating_add(1))],
                policy,
            )
            .is_err()
        );
    }
}
