use a3_application::{
    CommandAllowlistStore, CommandAllowlistStoreFailure, CommandAllowlistStoreVersion,
    ConfirmProjectCommandAllowlist, ConfirmProjectCommandAllowlistError, GetProjectSettings,
    GetProjectSettingsError, IndexPersistenceControl, IndexPersistenceControlError,
    KnowledgeIndexFailure, KnowledgeStoreFailure, ProjectCommandSettings, ProjectSettingsSnapshot,
};
use a3_domain::{
    AgentRunTimestamp, CommandCatalogId, DiscoveredCommandId, DiscoveredCommandKind, Progress,
    ProjectIdentity,
};
use a3_protocol::{
    ActiveProjectSettingsV1, CommandErrorV1, DiscoveredCommandKindV1, DiscoveredCommandV1,
    ErrorCodeV1, ProjectCommandConfirmationV1, ProjectCommandSettingsV1, ProjectIgnoreSettingsV1,
    ProjectSettingsResponseV1,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Reconstructs active-project Settings and commits exact catalog-bound selections.
#[derive(Debug, Clone)]
pub struct ProjectSettingsManager {
    query: GetProjectSettings,
    allowlist_store: Arc<dyn CommandAllowlistStore>,
}

impl ProjectSettingsManager {
    /// Wires the read use case and the existing append-only allowlist port.
    #[must_use]
    pub fn new(query: GetProjectSettings, allowlist_store: Arc<dyn CommandAllowlistStore>) -> Self {
        Self {
            query,
            allowlist_store,
        }
    }

    /// Reads ignore rules, published command evidence, and the current confirmation.
    pub async fn query(
        &self,
        project: &ProjectIdentity,
    ) -> Result<ProjectSettingsResponseV1, CommandErrorV1> {
        let snapshot = self
            .query
            .execute(project, &ProjectSettingsReadControl)
            .await
            .map_err(map_query_error)?;
        map_snapshot(&snapshot)
    }

    /// Rebuilds the current catalog, compares UI anchors, and appends one exact selection.
    pub async fn confirm(
        &self,
        project: &ProjectIdentity,
        expected_catalog_id: CommandCatalogId,
        expected_revision: Option<CommandAllowlistStoreVersion>,
        command_ids: Vec<DiscoveredCommandId>,
    ) -> Result<ProjectSettingsResponseV1, CommandErrorV1> {
        let snapshot = self
            .query
            .execute(project, &ProjectSettingsReadControl)
            .await
            .map_err(map_query_error)?;
        let command_settings = snapshot.commands().ok_or_else(invalid_request)?;
        if command_settings.catalog().id() != expected_catalog_id
            || command_settings
                .confirmation()
                .map(|stored| stored.version())
                != expected_revision
        {
            return Err(invalid_request());
        }
        let stored = ConfirmProjectCommandAllowlist::new(self.allowlist_store.as_ref())
            .execute(
                project,
                command_settings.catalog(),
                command_ids,
                now()?,
                expected_revision,
            )
            .await
            .map_err(map_confirmation_error)?;
        let updated = ProjectSettingsSnapshot::new(
            snapshot.ignore().clone(),
            Some(ProjectCommandSettings::new(
                command_settings.catalog().clone(),
                Some(stored),
            )),
        );
        map_snapshot(&updated)
    }
}

#[derive(Debug, Clone, Copy)]
struct ProjectSettingsReadControl;

impl IndexPersistenceControl for ProjectSettingsReadControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn report_progress(&self, _progress: Progress) -> Result<(), IndexPersistenceControlError> {
        Ok(())
    }
}

fn map_snapshot(
    snapshot: &ProjectSettingsSnapshot,
) -> Result<ProjectSettingsResponseV1, CommandErrorV1> {
    let ignore = ProjectIgnoreSettingsV1::new(
        snapshot.ignore().configuration_present(),
        snapshot.ignore().patterns().to_vec(),
    );
    let commands = match snapshot.commands() {
        None => ProjectCommandSettingsV1::NoPublishedIndex,
        Some(settings) => map_commands(settings)?,
    };
    Ok(ProjectSettingsResponseV1::available(
        ActiveProjectSettingsV1::new(ignore, commands),
    ))
}

fn map_commands(
    settings: &ProjectCommandSettings,
) -> Result<ProjectCommandSettingsV1, CommandErrorV1> {
    let selected = settings
        .current_confirmation()
        .map(|stored| stored.allowlist().command_ids())
        .unwrap_or_default();
    let commands = settings
        .catalog()
        .commands()
        .iter()
        .map(|command| {
            let evidence_count =
                u16::try_from(command.evidence().len()).map_err(|_| unavailable())?;
            Ok(DiscoveredCommandV1::new(
                encode_hex(command.id().as_bytes()),
                match command.kind() {
                    DiscoveredCommandKind::Test => DiscoveredCommandKindV1::Test,
                    DiscoveredCommandKind::Build => DiscoveredCommandKindV1::Build,
                    DiscoveredCommandKind::Lint => DiscoveredCommandKindV1::Lint,
                    DiscoveredCommandKind::Format => DiscoveredCommandKindV1::Format,
                },
                command
                    .working_directory()
                    .path()
                    .map(|path| encode_hex(path.as_bytes())),
                command.executable().as_str().to_owned(),
                command
                    .arguments()
                    .iter()
                    .map(|argument| argument.as_str().to_owned())
                    .collect(),
                evidence_count,
                selected.binary_search(&command.id()).is_ok(),
            ))
        })
        .collect::<Result<Vec<_>, CommandErrorV1>>()?;
    let confirmation = match settings.confirmation() {
        None => ProjectCommandConfirmationV1::NotConfirmed,
        Some(stored) if settings.current_confirmation().is_some() => {
            ProjectCommandConfirmationV1::Current {
                revision: stored.version().get().to_string(),
                confirmed_at_unix_millis: stored
                    .allowlist()
                    .confirmed_at()
                    .unix_millis()
                    .to_string(),
            }
        }
        Some(stored) => ProjectCommandConfirmationV1::Stale {
            revision: stored.version().get().to_string(),
            confirmed_at_unix_millis: stored.allowlist().confirmed_at().unix_millis().to_string(),
        },
    };
    Ok(ProjectCommandSettingsV1::Available {
        catalog_id: encode_hex(settings.catalog().id().as_bytes()),
        commands,
        confirmation,
    })
}

pub(crate) fn catalog_id_from_v1(value: &str) -> Result<CommandCatalogId, CommandErrorV1> {
    decode_id(value).map(CommandCatalogId::from_bytes)
}

pub(crate) fn command_ids_from_v1(
    values: &[String],
) -> Result<Vec<DiscoveredCommandId>, CommandErrorV1> {
    if values.is_empty() || values.len() > 256 {
        return Err(invalid_request());
    }
    values
        .iter()
        .map(|value| decode_id(value).map(DiscoveredCommandId::from_bytes))
        .collect()
}

pub(crate) fn allowlist_version_from_v1(
    value: Option<&str>,
) -> Result<Option<CommandAllowlistStoreVersion>, CommandErrorV1> {
    value.map(parse_positive_version).transpose()
}

fn parse_positive_version(value: &str) -> Result<CommandAllowlistStoreVersion, CommandErrorV1> {
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_request());
    }
    value
        .parse::<u64>()
        .ok()
        .and_then(|parsed| CommandAllowlistStoreVersion::new(parsed).ok())
        .ok_or_else(invalid_request)
}

fn decode_id(value: &str) -> Result<[u8; 32], CommandErrorV1> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_request());
    }
    let mut result = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or_else(invalid_request)?;
        let low = hex_nibble(pair[1]).ok_or_else(invalid_request)?;
        result[index] = (high << 4) | low;
    }
    Ok(result)
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn now() -> Result<AgentRunTimestamp, CommandErrorV1> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| unavailable())?;
    let millis = u64::try_from(duration.as_millis()).map_err(|_| unavailable())?;
    AgentRunTimestamp::from_unix_millis(millis).map_err(|_| unavailable())
}

fn map_query_error(error: GetProjectSettingsError) -> CommandErrorV1 {
    match error {
        GetProjectSettingsError::Ignore(
            a3_application::ProjectIgnoreSettingsSourceFailure::InvalidConfiguration,
        ) => invalid_request(),
        GetProjectSettingsError::Index(KnowledgeIndexFailure::Storage(storage)) => {
            map_storage_error(storage)
        }
        GetProjectSettingsError::Allowlist(storage) => map_allowlist_error(storage),
        GetProjectSettingsError::Ignore(
            a3_application::ProjectIgnoreSettingsSourceFailure::Unavailable,
        )
        | GetProjectSettingsError::Index(_)
        | GetProjectSettingsError::Discovery(_) => unavailable(),
    }
}

fn map_confirmation_error(error: ConfirmProjectCommandAllowlistError) -> CommandErrorV1 {
    match error {
        ConfirmProjectCommandAllowlistError::ProjectMismatch
        | ConfirmProjectCommandAllowlistError::InvalidConfirmation(_) => invalid_request(),
        ConfirmProjectCommandAllowlistError::Store(error) => map_allowlist_error(error),
    }
}

fn map_allowlist_error(error: CommandAllowlistStoreFailure) -> CommandErrorV1 {
    match error {
        CommandAllowlistStoreFailure::VersionConflict
        | CommandAllowlistStoreFailure::ProjectMismatch => invalid_request(),
        CommandAllowlistStoreFailure::Corrupt => {
            CommandErrorV1::project_settings(ErrorCodeV1::LocalStorageCorrupt)
        }
        CommandAllowlistStoreFailure::UnsupportedSchema => {
            CommandErrorV1::project_settings(ErrorCodeV1::LocalStorageUpgradeRequired)
        }
        CommandAllowlistStoreFailure::InvalidStoredData => {
            CommandErrorV1::project_settings(ErrorCodeV1::LocalStorageInvalidData)
        }
        CommandAllowlistStoreFailure::Unavailable => unavailable(),
    }
}

fn map_storage_error(error: KnowledgeStoreFailure) -> CommandErrorV1 {
    let code = match error {
        KnowledgeStoreFailure::Corrupt => ErrorCodeV1::LocalStorageCorrupt,
        KnowledgeStoreFailure::UnsupportedSchema => ErrorCodeV1::LocalStorageUpgradeRequired,
        KnowledgeStoreFailure::InvalidStoredData => ErrorCodeV1::LocalStorageInvalidData,
        KnowledgeStoreFailure::Unavailable | KnowledgeStoreFailure::IdentityConflict => {
            ErrorCodeV1::ProjectSettingsUnavailable
        }
    };
    CommandErrorV1::project_settings(code)
}

fn invalid_request() -> CommandErrorV1 {
    CommandErrorV1::project_settings(ErrorCodeV1::InvalidProjectSettingsRequest)
}

fn unavailable() -> CommandErrorV1 {
    CommandErrorV1::project_settings(ErrorCodeV1::ProjectSettingsUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{allowlist_version_from_v1, catalog_id_from_v1, command_ids_from_v1};
    use a3_protocol::ErrorCodeV1;

    #[test]
    fn untrusted_catalog_and_revision_anchors_are_canonical() {
        assert!(catalog_id_from_v1(&"ab".repeat(32)).is_ok());
        for invalid in ["ab".repeat(31), "AB".repeat(32), "gg".repeat(32)] {
            assert_eq!(
                catalog_id_from_v1(&invalid).map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidProjectSettingsRequest)
            );
        }
        assert_eq!(
            allowlist_version_from_v1(Some("1")).map(|value| value.map(|item| item.get())),
            Ok(Some(1))
        );
        for invalid in [Some("0"), Some("01"), Some("-1"), Some("")] {
            assert_eq!(
                allowlist_version_from_v1(invalid).map_err(|error| error.code()),
                Err(ErrorCodeV1::InvalidProjectSettingsRequest)
            );
        }
        assert!(command_ids_from_v1(&["cd".repeat(32)]).is_ok());
        assert!(command_ids_from_v1(&[]).is_err());
    }
}
