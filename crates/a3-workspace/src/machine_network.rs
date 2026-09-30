use a3_application::{
    AgentPermissionStore, AuthorizedMachineHttpGet, MachineHttpReceipt,
    MachineNetworkFailure as Failure, MachineNetworkFuture, MachineNetworkTool, ProcessRunControl,
};
use a3_domain::{
    MachineHttpAction, MachineHttpUrl, PatchFileContent, PolicyDecisionReason,
    PreparedMachineHttpAction, ProjectIdentity, SecretCandidateClassifierV1,
};
use std::{sync::Arc, time::Duration};

const RESPONSE_LIMIT: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Credential-free bounded GET adapter; inherited proxies, cookies and redirects are disabled.
#[derive(Debug)]
pub struct WorkspaceMachineNetworkTool {
    permissions: Arc<dyn AgentPermissionStore>,
    client: reqwest::Client,
}
impl WorkspaceMachineNetworkTool {
    /// Reuses the existing reqwest/rustls stack rather than adding another TLS implementation.
    pub fn new(permissions: Arc<dyn AgentPermissionStore>) -> Result<Self, Failure> {
        let client = reqwest::Client::builder()
            .retry(reqwest::retry::never())
            .no_proxy()
            .pool_max_idle_per_host(0)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| Failure::Unavailable)?;
        Ok(Self {
            permissions,
            client,
        })
    }
}

impl MachineNetworkTool for WorkspaceMachineNetworkTool {
    fn prepare(
        &self,
        project: &ProjectIdentity,
        action: &MachineHttpAction,
    ) -> Result<PreparedMachineHttpAction, Failure> {
        let url = validate_url(action.url().as_str())?;
        let url = MachineHttpUrl::new(url.to_string()).map_err(|_| Failure::Denied)?;
        Ok(PreparedMachineHttpAction::new(
            action.step_id(),
            url,
            project.worktree().id(),
        ))
    }

    fn get<'a>(
        &'a self,
        authorized: AuthorizedMachineHttpGet,
        control: &'a dyn ProcessRunControl,
    ) -> MachineNetworkFuture<'a, MachineHttpReceipt> {
        Box::pin(async move {
            if control.is_cancelled() {
                return Err(Failure::Cancelled);
            }
            // URL validation is repeated at the effect boundary, independently of the model.
            let url = validate_url(authorized.prepared().url().as_str())?;
            if authorized.decision().reason() == PolicyDecisionReason::SystemAutomatic {
                let current = self
                    .permissions
                    .load_agent_permissions()
                    .await
                    .map_err(|_| Failure::Denied)?;
                if authorized.decision().permission_settings() != Some(current) {
                    return Err(Failure::PermissionsChanged);
                }
            }
            if control.is_cancelled() {
                return Err(Failure::Cancelled);
            }
            let observe = async {
                let observation =
                    tokio::time::timeout(REQUEST_TIMEOUT, read_response(&self.client, url));
                tokio::select! {
                    biased;
                    _ = cancelled(control) => Err(Failure::Cancelled),
                    result = observation => result.map_err(|_| Failure::TimedOut)?,
                }
            };
            // A caller may have no reactor, or a Tokio context with I/O disabled. Never
            // inherit that assumption or nest block_on. This bounded worker is joined
            // on every exit; no detached task or connection survives its owned runtime.
            let (status, body) = std::thread::scope(|scope| {
                let worker = std::thread::Builder::new()
                    .name("a3-machine-http".to_owned())
                    .spawn_scoped(scope, move || {
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .map_err(|_| Failure::Unavailable)?
                            .block_on(observe)
                    })
                    .map_err(|_| Failure::Unavailable)?;
                worker.join().map_err(|_| Failure::Unavailable)?
            })?;
            let hash = body.content_hash();
            Ok(MachineHttpReceipt {
                resource: authorized.prepared().resource(),
                decision: authorized.decision().id(),
                status,
                body,
                hash,
            })
        })
    }
}

fn validate_url(value: &str) -> Result<reqwest::Url, Failure> {
    let url = reqwest::Url::parse(value).map_err(|_| Failure::Denied)?;
    let literal_loopback = url.host_str().is_some_and(|host| {
        host.trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
    });
    if (url.scheme() != "https" && !(url.scheme() == "http" && literal_loopback))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || SecretCandidateClassifierV1::classify(value).is_some()
    {
        return Err(Failure::Denied);
    }
    Ok(url)
}

async fn cancelled(control: &dyn ProcessRunControl) {
    loop {
        if control.is_cancelled() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn read_response(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> Result<(u16, PatchFileContent), Failure> {
    let mut response = client.get(url).send().await.map_err(|error| {
        if error.is_timeout() {
            Failure::TimedOut
        } else {
            Failure::Unavailable
        }
    })?;
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|n| n > RESPONSE_LIMIT as u64)
    {
        return Err(Failure::ResourceLimit);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unavailable)? {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_LIMIT {
            return Err(Failure::ResourceLimit);
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| Failure::Denied)?;
    if SecretCandidateClassifierV1::classify(text).is_some() {
        return Err(Failure::Denied);
    }
    let body = PatchFileContent::try_from_bytes(bytes).map_err(|_| Failure::Denied)?;
    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn targets_cannot_smuggle_credentials_redirects_or_plaintext_remote_transport() {
        for invalid in [
            "file:///secret",
            "http://example.com/",
            "https://user:password@example.com/",
            "https://example.com/#private",
            "http://localhost/",
        ] {
            assert_eq!(validate_url(invalid), Err(Failure::Denied));
        }
        for valid in [
            "https://example.com/info",
            "http://127.0.0.1:8080/",
            "http://[::1]:8080/",
        ] {
            assert!(validate_url(valid).is_ok());
        }
    }
}
