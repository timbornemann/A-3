//! Native A^3 desktop executable entry point.

fn main() -> Result<(), a3_desktop::DesktopRunError> {
    a3_desktop::run()
}

#[cfg(test)]
mod tests {
    use a3_desktop::CompositionRoot;
    use a3_domain::{ApplicationVersion, Platform};
    use a3_protocol::{
        CommandErrorV1, ErrorCodeV1, HealthResponseV1, HealthStatusV1, PlatformV1, ProtocolVersion,
    };
    use serde_json::json;
    use std::error::Error;
    use std::io;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;

    #[test]
    fn tauri_ipc_enforces_the_versioned_health_contract() -> Result<(), Box<dyn Error>> {
        let root = CompositionRoot::new(ApplicationVersion::try_from("1.2.3")?, Platform::Windows)?;
        let app = mock_builder()
            .manage(root)
            .invoke_handler(tauri::generate_handler![a3_desktop::commands::query_health])
            .build(tauri::generate_context!())?;
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build()?;
        let local_app_url = webview.url()?;
        let response = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_health".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": { "protocolVersion": 1 }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
        .map_err(|error| io::Error::other(error.to_string()))?
        .deserialize::<HealthResponseV1>()?;

        assert_eq!(response.protocol_version(), ProtocolVersion::V1);
        assert_eq!(response.application_version(), "1.2.3");
        assert_eq!(response.platform(), PlatformV1::Windows);
        assert_eq!(response.status(), HealthStatusV1::Ready);

        let invalid_payload = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_health".into(),
                callback: CallbackFn(2),
                error: CallbackFn(3),
                url: local_app_url.clone(),
                body: InvokeBody::Json(json!({
                    "request": {
                        "protocolVersion": 1,
                        "unexpected": true
                    }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        );

        assert!(invalid_payload.is_err());

        let unsupported_version = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "query_health".into(),
                callback: CallbackFn(4),
                error: CallbackFn(5),
                url: local_app_url,
                body: InvokeBody::Json(json!({
                    "request": { "protocolVersion": 999 }
                })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        );

        let error = match unsupported_version {
            Ok(_) => {
                return Err(io::Error::other("unsupported protocol version was accepted").into());
            }
            Err(error) => serde_json::from_value::<CommandErrorV1>(error)?,
        };
        assert_eq!(error.protocol_version(), ProtocolVersion::V1);
        assert_eq!(error.code(), ErrorCodeV1::UnsupportedProtocolVersion);

        Ok(())
    }
}
