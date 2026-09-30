//! Real loopback HTTP observations, stale authority and transport boundaries.
mod support;
use a3_application::*;
use a3_domain::*;
use a3_workspace::WorkspaceMachineNetworkTool;
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use support::TempDirectory;

#[derive(Debug)]
struct Permissions(Mutex<AgentPermissionSettings>);
impl AgentPermissionStore for Permissions {
    fn load_agent_permissions(&self) -> AgentPermissionFuture<'_> {
        Box::pin(async {
            self.0
                .lock()
                .map(|p| *p)
                .map_err(|_| AgentPermissionStoreFailure::Unavailable)
        })
    }
    fn update_agent_permissions(
        &self,
        expected: AgentPermissionRevision,
        mode: AgentPermissionMode,
    ) -> AgentPermissionFuture<'_> {
        Box::pin(async move {
            let mut p = self
                .0
                .lock()
                .map_err(|_| AgentPermissionStoreFailure::Unavailable)?;
            if p.revision() != expected {
                return Err(AgentPermissionStoreFailure::Conflict);
            }
            *p = AgentPermissionSettings::new(
                mode,
                expected
                    .next()
                    .map_err(|_| AgentPermissionStoreFailure::InvalidStoredData)?,
            );
            Ok(*p)
        })
    }
}
#[derive(Debug)]
struct Control(bool);
impl ProcessRunControl for Control {
    fn is_cancelled(&self) -> bool {
        self.0
    }
    fn wait_cancelled_timeout(&self, _: Duration) -> bool {
        self.0
    }
}

fn project(root: &std::path::Path) -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = CanonicalDirectory::from_canonicalized(fs::canonicalize(root)?)?;
    let repository = RepositoryId::from_bytes([1; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(repository, root.clone(), None),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([2; 32]),
            WorktreeAnchorId::from_bytes([3; 32]),
            repository,
            root,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}
fn decision(
    prepared: &PreparedMachineHttpAction,
    settings: AgentPermissionSettings,
) -> Result<PolicyDecision, Box<dyn Error>> {
    let time = AgentRunTimestamp::from_unix_millis(100)?;
    Ok(PolicyDecision::automatic(
        PolicyDecisionId::from_bytes([8; 32]),
        AgentRunId::from_bytes([5; 32]),
        &prepared.policy_action(),
        PolicyEvaluationTiming::new(time, time)?,
    )
    .with_permission_settings(settings))
}

#[test]
fn get_observes_actual_response_and_never_follows_a_redirect() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let fixture = TempDirectory::new()?;
    let project = project(fixture.path())?;
    let full = AgentPermissionSettings::new(
        AgentPermissionMode::FullMachine,
        AgentPermissionRevision::new(2)?,
    );
    let store = Arc::new(Permissions(Mutex::new(full)));
    let adapter = WorkspaceMachineNetworkTool::new(store)?;
    for (status, body, extra) in [
        (200, "Hello World", ""),
        (
            302,
            "Redirect was observed",
            "Location: https://invalid.invalid/\r\n",
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let action = MachineHttpAction::new(
            TaskStepId::from_bytes([4; 32]),
            MachineHttpUrl::new(format!("http://{}/safe", listener.local_addr()?))?,
        );
        let prepared = adapter.prepare(&project, &action)?;
        assert_eq!(
            AgentPermissionSettings::INITIAL.disposition(&prepared.policy_action()),
            PolicyDisposition::ApprovalRequired
        );
        assert!(
            AuthorizedMachineHttpGet::new(
                prepared.clone(),
                AgentRunId::from_bytes([5; 32]),
                &decision(&prepared, AgentPermissionSettings::INITIAL)?
            )
            .is_err()
        );
        let authorized = AuthorizedMachineHttpGet::new(
            prepared.clone(),
            AgentRunId::from_bytes([5; 32]),
            &decision(&prepared, full)?,
        )?;
        let result = std::thread::scope(|scope| {
            let worker = scope.spawn(move || -> std::io::Result<()> {
                let deadline = Instant::now() + Duration::from_secs(3);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                        Err(e) => return Err(e),
                    }
                };
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                let mut buffer = [0;8192];
                let read = stream.read(&mut buffer)?;
                let request = std::str::from_utf8(&buffer[..read]).map_err(std::io::Error::other)?;
                assert!(request.starts_with("GET /safe HTTP/1.1\r\n"));
                assert!(!request.to_lowercase().contains("authorization:"));
                write!(stream, "HTTP/1.1 {status} Result\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
                Ok(())
            });
            let observed = if status == 200 {
                futures::executor::block_on(adapter.get(authorized, &Control(false)))
            } else {
                runtime.block_on(adapter.get(authorized, &Control(false)))
            };
            let server = worker
                .join()
                .map_err(|_| std::io::Error::other("HTTP fixture worker failed"))?;
            server?;
            Ok::<_, Box<dyn Error>>(observed?)
        })?;
        assert_eq!(result.status, status);
        assert_eq!(result.body.as_bytes(), body.as_bytes());
        assert_eq!(result.hash, result.body.content_hash());
    }
    Ok(())
}

#[test]
fn permission_changes_and_cancellation_prevent_network_effects() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let fixture = TempDirectory::new()?;
    let project = project(fixture.path())?;
    let full = AgentPermissionSettings::new(
        AgentPermissionMode::FullMachine,
        AgentPermissionRevision::new(2)?,
    );
    let store = Arc::new(Permissions(Mutex::new(full)));
    let adapter = WorkspaceMachineNetworkTool::new(store.clone())?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let action = MachineHttpAction::new(
        TaskStepId::from_bytes([4; 32]),
        MachineHttpUrl::new(format!("http://{}/", listener.local_addr()?))?,
    );
    let prepared = adapter.prepare(&project, &action)?;
    let authorization = || {
        AuthorizedMachineHttpGet::new(
            prepared.clone(),
            AgentRunId::from_bytes([5; 32]),
            &decision(&prepared, full).map_err(|_| MachineNetworkFailure::Denied)?,
        )
    };
    assert!(matches!(
        runtime.block_on(adapter.get(authorization()?, &Control(true))),
        Err(MachineNetworkFailure::Cancelled)
    ));
    runtime.block_on(
        store.update_agent_permissions(full.revision(), AgentPermissionMode::AskPermissions),
    )?;
    assert!(matches!(
        runtime.block_on(adapter.get(authorization()?, &Control(false))),
        Err(MachineNetworkFailure::PermissionsChanged)
    ));
    assert_eq!(
        listener.accept().err().map(|e| e.kind()),
        Some(std::io::ErrorKind::WouldBlock)
    );
    Ok(())
}

#[test]
fn response_limits_and_secret_filter_never_admit_partial_or_sensitive_bodies()
-> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let project = project(fixture.path())?;
    let full = AgentPermissionSettings::new(
        AgentPermissionMode::FullMachine,
        AgentPermissionRevision::new(2)?,
    );
    for (body, expected) in [
        ("x".repeat(65537), MachineNetworkFailure::ResourceLimit),
        (
            format!("api_key=sk-{}", "A".repeat(64)),
            MachineNetworkFailure::Denied,
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let adapter = WorkspaceMachineNetworkTool::new(Arc::new(Permissions(Mutex::new(full))))?;
        let action = MachineHttpAction::new(
            TaskStepId::from_bytes([4; 32]),
            MachineHttpUrl::new(format!("http://{}/", listener.local_addr()?))?,
        );
        let prepared = adapter.prepare(&project, &action)?;
        let authorized = AuthorizedMachineHttpGet::new(
            prepared.clone(),
            AgentRunId::from_bytes([5; 32]),
            &decision(&prepared, full)?,
        )?;
        let result = std::thread::scope(|scope| -> Result<_, Box<dyn Error>> {
            let worker = scope.spawn(move || -> std::io::Result<()> {
                let deadline = Instant::now() + Duration::from_secs(3);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(value) => break value,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(e) => return Err(e),
                    }
                };
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                let mut request = [0; 8192];
                if stream.read(&mut request)? == 0 {
                    return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
                }
                // The client may close immediately after observing the oversized length.
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                Ok(())
            });
            let result = futures::executor::block_on(adapter.get(authorized, &Control(false)));
            worker
                .join()
                .map_err(|_| std::io::Error::other("fixture worker failed"))??;
            Ok(result)
        })?;
        assert_eq!(result.err(), Some(expected));
    }
    Ok(())
}

#[derive(Debug, Default)]
struct CancelControl(std::sync::atomic::AtomicBool);
impl ProcessRunControl for CancelControl {
    fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn wait_cancelled_timeout(&self, _: Duration) -> bool {
        self.is_cancelled()
    }
}
#[test]
fn cancellation_aborts_an_inflight_get_without_retrying() -> Result<(), Box<dyn Error>> {
    let fixture = TempDirectory::new()?;
    let project = project(fixture.path())?;
    let full = AgentPermissionSettings::new(
        AgentPermissionMode::FullMachine,
        AgentPermissionRevision::new(2)?,
    );
    let adapter = WorkspaceMachineNetworkTool::new(Arc::new(Permissions(Mutex::new(full))))?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let action = MachineHttpAction::new(
        TaskStepId::from_bytes([4; 32]),
        MachineHttpUrl::new(format!("http://{}/", listener.local_addr()?))?,
    );
    let prepared = adapter.prepare(&project, &action)?;
    let authorized = AuthorizedMachineHttpGet::new(
        prepared.clone(),
        AgentRunId::from_bytes([5; 32]),
        &decision(&prepared, full)?,
    )?;
    let control = CancelControl::default();
    std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
        let worker = scope.spawn(|| -> std::io::Result<()> {
            let deadline = Instant::now() + Duration::from_secs(3);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => return Err(e),
                }
            };
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            let mut request = [0; 8192];
            if stream.read(&mut request)? == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
            }
            control.0.store(true, std::sync::atomic::Ordering::SeqCst);
            // No response: only cancellation can produce the expected terminal result.
            let _ = stream.read(&mut request);
            assert!(listener.accept().is_err());
            Ok(())
        });
        let result = futures::executor::block_on(adapter.get(authorized, &control));
        assert_eq!(result.err(), Some(MachineNetworkFailure::Cancelled));
        worker
            .join()
            .map_err(|_| std::io::Error::other("fixture worker failed"))??;
        Ok(())
    })?;
    Ok(())
}
