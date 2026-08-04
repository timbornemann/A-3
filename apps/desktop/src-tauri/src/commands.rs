use crate::CompositionRoot;
use a3_protocol::{
    CommandErrorV1, HealthRequestV1, HealthResponseV1, OpenProjectRequestV1, OpenProjectResponseV1,
    ProtocolVersion,
};
use tauri::State;

#[tauri::command]
/// Opens one native directory picker and returns only a validated project identity projection.
pub async fn open_project(
    request: OpenProjectRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<OpenProjectResponseV1, CommandErrorV1> {
    execute_open_project(request, root.inner())
}

#[tauri::command]
/// Returns process health metadata when the request uses the current protocol version.
pub fn query_health(
    request: HealthRequestV1,
    root: State<'_, CompositionRoot>,
) -> Result<HealthResponseV1, CommandErrorV1> {
    execute_query_health(request, root.inner())
}

fn execute_query_health(
    request: HealthRequestV1,
    root: &CompositionRoot,
) -> Result<HealthResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    Ok(root.query_health())
}

fn execute_open_project(
    request: OpenProjectRequestV1,
    root: &CompositionRoot,
) -> Result<OpenProjectResponseV1, CommandErrorV1> {
    if request.protocol_version() != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }

    root.open_project()
}

#[cfg(test)]
mod tests {
    use super::{execute_open_project, execute_query_health};
    use crate::CompositionRoot;
    use a3_application::{ProjectDirectoryPicker, ProjectDirectorySelectionError};
    use a3_domain::{ApplicationVersion, Platform};
    use a3_protocol::{ErrorCodeV1, HealthRequestV1, OpenProjectRequestV1, ProtocolVersion};
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Debug)]
    struct CancelledPicker;

    impl ProjectDirectoryPicker for CancelledPicker {
        fn pick_project_directory(
            &self,
        ) -> Result<Option<PathBuf>, ProjectDirectorySelectionError> {
            Ok(None)
        }
    }

    fn root() -> Result<CompositionRoot, Box<dyn std::error::Error>> {
        Ok(CompositionRoot::new(
            ApplicationVersion::try_from("0.1.0")?,
            Platform::Windows,
            Arc::new(CancelledPicker),
        )?)
    }

    #[test]
    fn rejects_unsupported_protocol_version_without_executing_payload()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result = execute_query_health(HealthRequestV1::new(ProtocolVersion::new(999)), &root);

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }

    #[test]
    fn project_command_rejects_unsupported_version_before_opening_picker()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = root()?;

        let result =
            execute_open_project(OpenProjectRequestV1::new(ProtocolVersion::new(999)), &root);

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }
}
