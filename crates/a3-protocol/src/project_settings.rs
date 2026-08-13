use crate::ProtocolVersion;
use serde::de::{Error as DeError, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Pathless read request for active-project ignore and safe-command Settings.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryProjectSettingsRequestV1 {
    protocol_version: ProtocolVersion,
}

impl QueryProjectSettingsRequestV1 {
    /// Returns the schema version checked before Core-owned project selection.
    #[must_use]
    pub const fn protocol_version(self) -> ProtocolVersion {
        self.protocol_version
    }
}

/// Exact catalog-bound selection without project, argv, evidence, or timestamp authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ConfirmProjectCommandAllowlistRequestV1 {
    protocol_version: ProtocolVersion,
    expected_catalog_id: String,
    expected_allowlist_revision: Option<String>,
    #[serde(deserialize_with = "deserialize_command_ids")]
    command_ids: Vec<String>,
}

fn deserialize_command_ids<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct CommandIdsVisitor;

    impl<'de> Visitor<'de> for CommandIdsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("between one and 256 lowercase command IDs")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            if sequence.size_hint().is_some_and(|hint| hint > 256) {
                return Err(A::Error::custom("too many command IDs"));
            }
            let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(256));
            while let Some(value) = sequence.next_element::<String>()? {
                if values.len() == 256 {
                    return Err(A::Error::custom("too many command IDs"));
                }
                if value.len() != 64
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                {
                    return Err(A::Error::custom("invalid command ID"));
                }
                values.push(value);
            }
            if values.is_empty() {
                return Err(A::Error::custom("command selection is empty"));
            }
            Ok(values)
        }
    }

    deserializer.deserialize_seq(CommandIdsVisitor)
}

impl ConfirmProjectCommandAllowlistRequestV1 {
    /// Returns the request schema version.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the exact current catalog identity shown by the UI.
    #[must_use]
    pub fn expected_catalog_id(&self) -> &str {
        &self.expected_catalog_id
    }

    /// Returns the latest visible append-only confirmation revision, if one exists.
    #[must_use]
    pub fn expected_allowlist_revision(&self) -> Option<&str> {
        self.expected_allowlist_revision.as_deref()
    }

    /// Returns only selected current command IDs; argv remains Core-owned.
    #[must_use]
    pub fn command_ids(&self) -> &[String] {
        &self.command_ids
    }
}

/// Read-only validated `.a3/project.toml` exclusion projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectIgnoreSettingsV1 {
    configuration_present: bool,
    patterns: Vec<String>,
}

impl ProjectIgnoreSettingsV1 {
    /// Creates a bounded projection after adapter and Application validation.
    #[must_use]
    pub const fn new(configuration_present: bool, patterns: Vec<String>) -> Self {
        Self {
            configuration_present,
            patterns,
        }
    }
}

/// Closed category of a manifest-evidenced direct-argv command.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveredCommandKindV1 {
    /// Project test command.
    Test,
    /// Project build command.
    Build,
    /// Static diagnostic command.
    Lint,
    /// Formatting check command.
    Format,
}

/// One exact bounded direct-argv template from current published evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DiscoveredCommandV1 {
    command_id: String,
    kind: DiscoveredCommandKindV1,
    working_directory_hex: Option<String>,
    executable: String,
    arguments: Vec<String>,
    evidence_count: u16,
    selected: bool,
}

impl DiscoveredCommandV1 {
    /// Creates a safe display projection from the current Core-owned catalog.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        command_id: String,
        kind: DiscoveredCommandKindV1,
        working_directory_hex: Option<String>,
        executable: String,
        arguments: Vec<String>,
        evidence_count: u16,
        selected: bool,
    ) -> Self {
        Self {
            command_id,
            kind,
            working_directory_hex,
            executable,
            arguments,
            evidence_count,
            selected,
        }
    }
}

/// Durable confirmation state relative to the current evidence-sensitive catalog.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ProjectCommandConfirmationV1 {
    /// No command subset has ever been confirmed for this worktree.
    NotConfirmed,
    /// Latest confirmation exactly matches current manifest evidence.
    Current {
        /// Monotone private-store CAS revision.
        revision: String,
        /// Core-generated confirmation time.
        #[serde(rename = "confirmedAtUnixMillis")]
        confirmed_at_unix_millis: String,
    },
    /// A prior selection exists but manifest evidence changed.
    Stale {
        /// Monotone private-store CAS revision required to replace it.
        revision: String,
        /// Core-generated time of the now-stale confirmation.
        #[serde(rename = "confirmedAtUnixMillis")]
        confirmed_at_unix_millis: String,
    },
}

/// Command Settings await a publication or present the complete current bounded catalog.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ProjectCommandSettingsV1 {
    /// The worktree has not published its first deterministic index yet.
    NoPublishedIndex,
    /// A current command catalog and its confirmation state are available.
    Available {
        /// Evidence-sensitive identity of the complete displayed catalog.
        #[serde(rename = "catalogId")]
        catalog_id: String,
        /// At most 256 canonical command templates.
        commands: Vec<DiscoveredCommandV1>,
        /// Latest durable selection relative to this catalog.
        confirmation: ProjectCommandConfirmationV1,
    },
}

/// Complete project-owned Settings projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ActiveProjectSettingsV1 {
    ignore: ProjectIgnoreSettingsV1,
    commands: ProjectCommandSettingsV1,
}

impl ActiveProjectSettingsV1 {
    /// Groups the two project settings domains without exposing a project path or identity.
    #[must_use]
    pub const fn new(ignore: ProjectIgnoreSettingsV1, commands: ProjectCommandSettingsV1) -> Self {
        Self { ignore, commands }
    }
}

/// Core-owned project selection result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "status")]
pub enum ProjectSettingsResultV1 {
    /// No validated worktree is active.
    NoProject,
    /// Dedicated project Settings were reconstructed safely.
    Available {
        /// Complete bounded settings projection.
        settings: Box<ActiveProjectSettingsV1>,
    },
}

/// Versioned project Settings query or mutation response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectSettingsResponseV1 {
    protocol_version: ProtocolVersion,
    result: ProjectSettingsResultV1,
}

impl ProjectSettingsResponseV1 {
    /// Creates the pathless no-project response.
    #[must_use]
    pub const fn no_project() -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ProjectSettingsResultV1::NoProject,
        }
    }

    /// Creates a complete active-project response.
    #[must_use]
    pub fn available(settings: ActiveProjectSettingsV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result: ProjectSettingsResultV1::Available {
                settings: Box::new(settings),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConfirmProjectCommandAllowlistRequestV1;

    #[test]
    fn confirmation_request_rejects_core_owned_authority() {
        for forbidden in [
            "projectId",
            "worktreeId",
            "executable",
            "arguments",
            "confirmedAtUnixMillis",
        ] {
            let mut value = serde_json::json!({
                "protocolVersion": 1,
                "expectedCatalogId": "11".repeat(32),
                "expectedAllowlistRevision": null,
                "commandIds": ["22".repeat(32)]
            });
            value[forbidden] = serde_json::json!("forbidden");
            assert!(
                serde_json::from_value::<ConfirmProjectCommandAllowlistRequestV1>(value).is_err()
            );
        }
    }
}
