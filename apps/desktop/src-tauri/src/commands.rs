use crate::CompositionRoot;
use a3_protocol::{CommandErrorV1, HealthRequestV1, HealthResponseV1, ProtocolVersion};
use tauri::State;

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

#[cfg(test)]
mod tests {
    use super::execute_query_health;
    use crate::CompositionRoot;
    use a3_domain::{ApplicationVersion, ApplicationVersionError, Platform};
    use a3_protocol::{ErrorCodeV1, HealthRequestV1, ProtocolVersion};

    #[test]
    fn rejects_unsupported_protocol_version_without_executing_payload()
    -> Result<(), ApplicationVersionError> {
        let root = CompositionRoot::new(ApplicationVersion::try_from("0.1.0")?, Platform::Windows);

        let result = execute_query_health(HealthRequestV1::new(ProtocolVersion::new(999)), &root);

        assert_eq!(
            result.map_err(|error| error.code()),
            Err(ErrorCodeV1::UnsupportedProtocolVersion)
        );
        Ok(())
    }
}
