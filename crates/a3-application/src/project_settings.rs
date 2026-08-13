use crate::{
    CommandAllowlistStore, CommandAllowlistStoreFailure, DiscoverProjectCommands,
    KnowledgeIndexFailure, KnowledgeIndexStore, StoredProjectCommandAllowlist,
};
use a3_domain::{ProjectCommandCatalog, ProjectIdentity};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MAX_IGNORE_PATTERNS: usize = 256;
const MAX_IGNORE_PATTERN_BYTES: usize = 1_024;

/// Validated read-only projection of repository-owned exclusion rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIgnoreSettings {
    configuration_present: bool,
    patterns: Vec<String>,
}

impl ProjectIgnoreSettings {
    /// Revalidates the bounded adapter output before it enters a product projection.
    pub fn try_new(
        configuration_present: bool,
        patterns: Vec<String>,
    ) -> Result<Self, ProjectIgnoreSettingsError> {
        if !configuration_present && !patterns.is_empty() {
            return Err(ProjectIgnoreSettingsError::PatternsWithoutConfiguration);
        }
        if patterns.len() > MAX_IGNORE_PATTERNS {
            return Err(ProjectIgnoreSettingsError::TooManyPatterns {
                actual: patterns.len(),
            });
        }
        if patterns.iter().any(|pattern| {
            pattern.is_empty()
                || pattern.len() > MAX_IGNORE_PATTERN_BYTES
                || pattern.chars().any(char::is_control)
        }) {
            return Err(ProjectIgnoreSettingsError::InvalidPattern);
        }
        Ok(Self {
            configuration_present,
            patterns,
        })
    }

    /// Returns whether `.a3/project.toml` exists as a validated regular file.
    #[must_use]
    pub const fn configuration_present(&self) -> bool {
        self.configuration_present
    }

    /// Returns the validated exclusion-only patterns in repository order.
    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }
}

/// Invalid projection returned by the privileged project-config adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectIgnoreSettingsError {
    /// A missing configuration cannot own patterns.
    PatternsWithoutConfiguration,
    /// More patterns were returned than the V1 discovery policy permits.
    TooManyPatterns {
        /// Observed number of patterns.
        actual: usize,
    },
    /// A pattern was empty, oversized, or contained control data.
    InvalidPattern,
}

impl fmt::Display for ProjectIgnoreSettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PatternsWithoutConfiguration => {
                "project ignore patterns require a present configuration"
            }
            Self::TooManyPatterns { .. } => "project ignore pattern count exceeds the V1 bound",
            Self::InvalidPattern => "project ignore pattern is invalid",
        })
    }
}

impl Error for ProjectIgnoreSettingsError {}

/// Future returned by the narrow, read-only project configuration boundary.
pub type ProjectIgnoreSettingsFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ProjectIgnoreSettings, ProjectIgnoreSettingsSourceFailure>>
            + Send
            + 'a,
    >,
>;

/// Reads only the dedicated, validated repository-owned project configuration.
pub trait ProjectIgnoreSettingsSource: fmt::Debug + Send + Sync {
    /// Reads exclusion rules for the already validated Core-owned worktree.
    fn read<'a>(&'a self, project: &'a ProjectIdentity) -> ProjectIgnoreSettingsFuture<'a>;
}

/// Stable failure at the dedicated project-config adapter boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectIgnoreSettingsSourceFailure {
    /// The file changed, was malformed, or violated exclusion-only policy.
    InvalidConfiguration,
    /// The validated worktree or configuration file could not be read safely.
    Unavailable,
}

impl fmt::Display for ProjectIgnoreSettingsSourceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidConfiguration => "project configuration is invalid",
            Self::Unavailable => "project configuration is unavailable",
        })
    }
}

impl Error for ProjectIgnoreSettingsSourceFailure {}

/// Current command catalog plus the latest durable user confirmation, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCommandSettings {
    catalog: ProjectCommandCatalog,
    confirmation: Option<StoredProjectCommandAllowlist>,
}

impl ProjectCommandSettings {
    /// Groups one current evidence-derived catalog with the latest append-only selection.
    #[must_use]
    pub const fn new(
        catalog: ProjectCommandCatalog,
        confirmation: Option<StoredProjectCommandAllowlist>,
    ) -> Self {
        Self {
            catalog,
            confirmation,
        }
    }

    /// Returns the current deterministic catalog.
    #[must_use]
    pub const fn catalog(&self) -> &ProjectCommandCatalog {
        &self.catalog
    }

    /// Returns the latest stored selection even when its evidence-bound catalog is stale.
    #[must_use]
    pub const fn confirmation(&self) -> Option<&StoredProjectCommandAllowlist> {
        self.confirmation.as_ref()
    }

    /// Returns the selected command IDs only when worktree and catalog still match exactly.
    #[must_use]
    pub fn current_confirmation(&self) -> Option<&StoredProjectCommandAllowlist> {
        self.confirmation.as_ref().filter(|stored| {
            stored.allowlist().worktree_id() == self.catalog.worktree_id()
                && stored.allowlist().catalog_id() == self.catalog.id()
        })
    }

    /// Returns whether a durable selection exists but current manifest evidence superseded it.
    #[must_use]
    pub fn confirmation_is_stale(&self) -> bool {
        self.confirmation.is_some() && self.current_confirmation().is_none()
    }
}

/// Complete active-project Settings read; command discovery awaits a published index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSettingsSnapshot {
    ignore: ProjectIgnoreSettings,
    commands: Option<ProjectCommandSettings>,
}

impl ProjectSettingsSnapshot {
    /// Creates a complete bounded read-only product snapshot.
    #[must_use]
    pub const fn new(
        ignore: ProjectIgnoreSettings,
        commands: Option<ProjectCommandSettings>,
    ) -> Self {
        Self { ignore, commands }
    }

    /// Returns repository-owned, exclusion-only ignore settings.
    #[must_use]
    pub const fn ignore(&self) -> &ProjectIgnoreSettings {
        &self.ignore
    }

    /// Returns the catalog and confirmation, or `None` before the first published index.
    #[must_use]
    pub const fn commands(&self) -> Option<&ProjectCommandSettings> {
        self.commands.as_ref()
    }
}

/// Loads repository Settings only from dedicated config, published evidence, and private storage.
#[derive(Debug, Clone)]
pub struct GetProjectSettings {
    ignore_source: Arc<dyn ProjectIgnoreSettingsSource>,
    index_store: Arc<dyn KnowledgeIndexStore>,
    allowlist_store: Arc<dyn CommandAllowlistStore>,
}

impl GetProjectSettings {
    /// Wires the three narrow read capabilities used by this projection.
    #[must_use]
    pub fn new(
        ignore_source: Arc<dyn ProjectIgnoreSettingsSource>,
        index_store: Arc<dyn KnowledgeIndexStore>,
        allowlist_store: Arc<dyn CommandAllowlistStore>,
    ) -> Self {
        Self {
            ignore_source,
            index_store,
            allowlist_store,
        }
    }

    /// Reads one coherent current presentation without interpreting normal repository text.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        control: &dyn crate::IndexPersistenceControl,
    ) -> Result<ProjectSettingsSnapshot, GetProjectSettingsError> {
        let ignore = self
            .ignore_source
            .read(project)
            .await
            .map_err(GetProjectSettingsError::Ignore)?;
        let published = self
            .index_store
            .latest_published_index(project, control)
            .await
            .map_err(GetProjectSettingsError::Index)?;
        let Some(published) = published else {
            return Ok(ProjectSettingsSnapshot::new(ignore, None));
        };
        let catalog = DiscoverProjectCommands
            .execute(project.worktree().id(), &published)
            .map_err(GetProjectSettingsError::Discovery)?;
        let confirmation = self
            .allowlist_store
            .load_current(project)
            .await
            .map_err(GetProjectSettingsError::Allowlist)?;
        Ok(ProjectSettingsSnapshot::new(
            ignore,
            Some(ProjectCommandSettings::new(catalog, confirmation)),
        ))
    }
}

/// Active-project Settings could not be reconstructed safely.
#[derive(Debug)]
pub enum GetProjectSettingsError {
    /// Dedicated project configuration was invalid or unavailable.
    Ignore(ProjectIgnoreSettingsSourceFailure),
    /// Latest published index could not be read.
    Index(KnowledgeIndexFailure),
    /// Published manifest evidence violated command discovery invariants.
    Discovery(crate::CommandDiscoveryFailure),
    /// Current private allowlist confirmation could not be read.
    Allowlist(CommandAllowlistStoreFailure),
}

impl fmt::Display for GetProjectSettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ignore(_) => "project ignore settings could not be read",
            Self::Index(_) => "published index could not be read for project settings",
            Self::Discovery(_) => "safe command catalog could not be derived",
            Self::Allowlist(_) => "project command allowlist could not be read",
        })
    }
}

impl Error for GetProjectSettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ignore(source) => Some(source),
            Self::Index(source) => Some(source),
            Self::Discovery(source) => Some(source),
            Self::Allowlist(source) => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProjectIgnoreSettings, ProjectIgnoreSettingsError};

    #[test]
    fn ignore_projection_is_bounded_and_missing_configuration_is_empty() {
        assert_eq!(
            ProjectIgnoreSettings::try_new(false, vec!["target/**".to_owned()]),
            Err(ProjectIgnoreSettingsError::PatternsWithoutConfiguration)
        );
        assert!(ProjectIgnoreSettings::try_new(false, Vec::new()).is_ok());
        assert!(ProjectIgnoreSettings::try_new(true, vec!["generated/**".to_owned()]).is_ok());
        assert_eq!(
            ProjectIgnoreSettings::try_new(true, vec!["bad\npattern".to_owned()]),
            Err(ProjectIgnoreSettingsError::InvalidPattern)
        );
    }
}
