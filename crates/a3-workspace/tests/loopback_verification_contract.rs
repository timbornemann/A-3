//! Exercises the real authorized process, held HTTP connection and cleanup.

#![cfg(any(windows, target_os = "linux"))]

mod support;

use a3_application::{
    AuthorizedLoopbackVerification, AuthorizedProcessSpec, LoopbackCleanupProof, LoopbackMethod,
    LoopbackProbe, LoopbackVerificationFailure, LoopbackVerificationOutcome,
    LoopbackVerificationRunner, PreparedLoopbackVerification, ProcessRunControl,
};
use a3_domain::{
    AgentRunId, AgentRunTimestamp, ApprovalGrant, ApprovalId, ApprovalRequest, ApprovalRequestId,
    CanonicalDirectory, ContentHash, FileRevision, GitHead, GitReferenceName, LoopbackExpectedBody,
    LoopbackHeaderChecks, LoopbackHttpCheck, LoopbackHttpRecipe, LoopbackStart, PolicyAction,
    PolicyDecision, PolicyDecisionId, PolicyEvaluationTiming, ProcessArgument,
    ProcessEnvironmentVariable, ProcessExecutable, ProcessExecutionMode, ProcessNetworkScope,
    ProcessOutputLimit, ProcessPlanBinding, ProcessSpec, ProcessSpecSchemaVersion, ProcessTimeout,
    ProjectIdentity, RepositoryId, RepositoryIdentity, RepositoryPath, SnapshotId, TaskStepId,
    WorkspaceDirectory, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use a3_workspace::{ProcessHostEnvironment, WorkspaceLoopbackVerificationRunner};
use std::error::Error;
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use support::TempDirectory;

const RUN: AgentRunId = AgentRunId::from_bytes([20; 32]);
const SERVER: &str = r#"
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from time import sleep
import argparse
import os

class Handler(BaseHTTPRequestHandler):
    def respond(self, status, body=b'', **headers):
        self.send_response(status)
        for key, value in headers.items():
            self.send_header(key.replace('_', '-'), value)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == '/health':
            self.respond(200, b'Healthy\n', Content_Type='text/plain; charset=utf-8')
        elif self.path == '/redirect':
            self.respond(302, Location='/health')
        elif self.path == '/slow':
            Path('http-entered').touch()
            sleep(30)
        elif self.path == '/overlong':
            self.respond(200, b'x' * 8193)
        elif self.path == '/invalid-utf8':
            self.respond(200, b'\xff', Content_Type='text/plain; charset=utf-8')
        elif self.path == '/self-stop':
            self.server.server_close()
            self.respond(200)
            os._exit(0)
        else:
            self.respond(404)

    def reject(self):
        self.respond(405, Allow='GET')

    do_POST = do_PUT = do_DELETE = do_PATCH = reject

parser = argparse.ArgumentParser()
parser.add_argument('--host')
parser.add_argument('--port', type=int)
args = parser.parse_args()
Path('started').touch()
with HTTPServer((args.host, args.port), Handler) as server:
    server.serve_forever()
"#;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[test]
fn observes_exact_get_and_negative_requests_then_stops_the_owned_service() -> TestResult {
    let fixture = Fixture::new()?;
    let port = unused_port()?;
    let authorized = fixture.authorized(
        port,
        &[
            (LoopbackMethod::Get, "/health"),
            (LoopbackMethod::Get, "/missing"),
            (LoopbackMethod::Post, "/health"),
            (LoopbackMethod::Put, "/health"),
            (LoopbackMethod::Delete, "/health"),
            (LoopbackMethod::Patch, "/health"),
            (LoopbackMethod::Get, "/redirect"),
            (LoopbackMethod::Get, "/invalid-utf8"),
        ],
    )?;
    assert!(fixture.runner.supports_owned_cleanup());
    let outcome = futures::executor::block_on(fixture.runner.verify(
        &fixture.project,
        authorized,
        &Control::default(),
    ));
    let LoopbackVerificationOutcome::Stopped { cleanup, result } = outcome else {
        return Err(format!("expected observed cleanup, got {outcome:?}").into());
    };
    assert_eq!(cleanup, expected_cleanup());
    let report = result?;
    assert_eq!(report.start_decision, decision_id(1));
    assert_eq!(
        report
            .probes
            .iter()
            .map(|row| row.status())
            .collect::<Vec<_>>(),
        [200, 404, 405, 405, 405, 405, 302, 200]
    );
    assert_eq!(report.probes[0].body_bytes(), 8);
    assert_eq!(report.probes[0].body_hash(), hash(b"Healthy\n"));
    assert!(report.probes[0].headers().plain_utf8);
    for row in &report.probes[2..6] {
        assert!(row.headers().allow_get);
    }
    assert!(!report.probes[7].headers().plain_utf8);
    assert_eq!(report.probes[7].body_hash(), hash(b"\xff"));
    assert_stopped(port);
    Ok(())
}

#[test]
fn occupied_port_and_changed_script_are_denied_before_start() -> TestResult {
    let fixture = Fixture::new()?;
    let foreign = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = NonZeroU16::new(foreign.local_addr()?.port()).ok_or("invalid port")?;
    let authorized = fixture.authorized(port, &[(LoopbackMethod::Get, "/health")])?;
    assert!(matches!(
        futures::executor::block_on(fixture.runner.verify(
            &fixture.project,
            authorized,
            &Control::default(),
        )),
        LoopbackVerificationOutcome::NotStarted(LoopbackVerificationFailure::Occupied)
    ));
    foreign.set_nonblocking(true)?;
    assert!(
        matches!(foreign.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    assert!(!fixture.temp.path().join("started").exists());

    drop(foreign);
    let cancelled = Control(AtomicBool::new(true));
    let authorized = fixture.authorized(port, &[(LoopbackMethod::Get, "/health")])?;
    assert!(matches!(
        futures::executor::block_on(fixture.runner.verify(
            &fixture.project,
            authorized,
            &cancelled
        )),
        LoopbackVerificationOutcome::NotStarted(LoopbackVerificationFailure::Cancelled)
    ));
    assert!(!fixture.temp.path().join("started").exists());

    let authorized = fixture.authorized(port, &[(LoopbackMethod::Get, "/health")])?;
    fs::write(
        fixture.temp.path().join("server.py"),
        "raise RuntimeError()\n",
    )?;
    assert!(matches!(
        futures::executor::block_on(fixture.runner.verify(
            &fixture.project,
            authorized,
            &Control::default(),
        )),
        LoopbackVerificationOutcome::NotStarted(LoopbackVerificationFailure::StaleScript)
    ));
    assert!(!fixture.temp.path().join("started").exists());
    Ok(())
}

#[test]
fn a_process_approval_for_a_different_argv_port_cannot_start_the_recipe() -> TestResult {
    let fixture = Fixture::new()?;
    let expected = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let other = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = NonZeroU16::new(expected.local_addr()?.port()).ok_or("invalid fixture port")?;
    let intended = fixture.authorized(port, &[(LoopbackMethod::Get, "/health")])?;
    let intended = intended.prepared();
    let wrong_port = other.local_addr()?.port();
    assert_ne!(port.get(), wrong_port);
    let wrong_port = wrong_port.to_string();
    let wrong_start = fixture.start_spec(
        port,
        12_000,
        &[
            "-B",
            "server.py",
            "--host",
            "127.0.0.1",
            "--port",
            &wrong_port,
        ],
    )?;
    let approval = approve(&wrong_start.policy_action(), 10)?;
    let ordinary_process = AuthorizedProcessSpec::new(wrong_start, &approval)?;
    let (approved_start, _) = ordinary_process.into_parts();
    // A valid process approval does not authorize different argv for this recipe.
    assert!(matches!(
        PreparedLoopbackVerification::new(
            approved_start,
            intended.script().clone(),
            intended.snapshot(),
            intended.recipe().clone(),
        ),
        Err(LoopbackVerificationFailure::Denied)
    ));
    assert!(!fixture.temp.path().join("started").exists());
    for listener in [expected, other] {
        listener.set_nonblocking(true)?;
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
    Ok(())
}

#[test]
fn cancellation_during_the_owned_request_still_closes_the_listener() -> TestResult {
    let fixture = Fixture::new()?;
    let port = unused_port()?;
    let authorized = fixture.authorized(port, &[(LoopbackMethod::Get, "/slow")])?;
    let control = Control::default();
    let entered = fixture.temp.path().join("http-entered");
    let result = thread::scope(|scope| {
        scope.spawn(|| {
            let until = Instant::now() + Duration::from_secs(6);
            while !entered.exists() && Instant::now() < until {
                thread::sleep(Duration::from_millis(10));
            }
            control.0.store(true, Ordering::Relaxed);
        });
        futures::executor::block_on(
            fixture
                .runner
                .verify(&fixture.project, authorized, &control),
        )
    });
    assert!(entered.exists());
    assert_eq!(
        result,
        LoopbackVerificationOutcome::Stopped {
            cleanup: expected_cleanup(),
            result: Err(LoopbackVerificationFailure::Cancelled),
        }
    );
    assert_stopped(port);
    Ok(())
}

#[test]
fn timeout_and_response_limit_still_close_the_owned_listener() -> TestResult {
    let fixture = Fixture::new()?;
    for (path, timeout, expected) in [
        ("/slow", 8_000, LoopbackVerificationFailure::TimedOut),
        (
            "/overlong",
            12_000,
            LoopbackVerificationFailure::ResourceLimit,
        ),
    ] {
        let port = unused_port()?;
        let authorized =
            fixture.authorized_with_timeout(port, &[(LoopbackMethod::Get, path)], timeout)?;
        let result = futures::executor::block_on(fixture.runner.verify(
            &fixture.project,
            authorized,
            &Control::default(),
        ));
        assert_eq!(
            result,
            LoopbackVerificationOutcome::Stopped {
                cleanup: expected_cleanup(),
                result: Err(expected),
            },
            "{path}"
        );
        assert_stopped(port);
    }
    assert!(fixture.temp.path().join("http-entered").exists());
    Ok(())
}

#[test]
fn a_response_from_a_service_that_stops_itself_does_not_finish_the_recipe() -> TestResult {
    let fixture = Fixture::new()?;
    let port = unused_port()?;
    let authorized = fixture.authorized(port, &[(LoopbackMethod::Get, "/self-stop")])?;
    let result = futures::executor::block_on(fixture.runner.verify(
        &fixture.project,
        authorized,
        &Control::default(),
    ));
    assert!(fixture.temp.path().join("started").exists());
    assert!(matches!(
        result,
        LoopbackVerificationOutcome::Stopped { result: Err(_), .. }
            | LoopbackVerificationOutcome::UnknownCleanup
    ));
    assert_stopped(port);
    Ok(())
}

const fn expected_cleanup() -> LoopbackCleanupProof {
    if cfg!(target_os = "linux") {
        LoopbackCleanupProof::ProcessGroupAndListener
    } else {
        LoopbackCleanupProof::WindowsJobAndListener
    }
}

#[derive(Debug, Default)]
struct Control(AtomicBool);

impl ProcessRunControl for Control {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn wait_cancelled_timeout(&self, time: Duration) -> bool {
        thread::sleep(time);
        self.is_cancelled()
    }
}

struct Fixture {
    temp: TempDirectory,
    project: ProjectIdentity,
    runner: WorkspaceLoopbackVerificationRunner,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let temp = TempDirectory::new()?;
        fs::write(temp.path().join("server.py"), SERVER)?;
        let root = CanonicalDirectory::from_canonicalized(fs::canonicalize(temp.path())?)?;
        let repository = RepositoryId::from_bytes([1; 32]);
        let project = ProjectIdentity::new(
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
        )?;
        let mut host_names = child_environment()?;
        host_names.push(ProcessEnvironmentVariable::try_from_string(
            "PATH".to_owned(),
        )?);
        let environment = ProcessHostEnvironment::capture(host_names)?;
        Ok(Self {
            temp,
            project,
            runner: WorkspaceLoopbackVerificationRunner::new(environment),
        })
    }

    fn authorized(
        &self,
        port: NonZeroU16,
        requests: &[(LoopbackMethod, &str)],
    ) -> TestResult<AuthorizedLoopbackVerification> {
        self.authorized_with_timeout(port, requests, 12_000)
    }

    fn authorized_with_timeout(
        &self,
        port: NonZeroU16,
        requests: &[(LoopbackMethod, &str)],
        timeout_millis: u64,
    ) -> TestResult<AuthorizedLoopbackVerification> {
        let start = self.start_spec(
            port,
            timeout_millis,
            &[
                "-B",
                "server.py",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
            ],
        )?;
        let start_decision = approve(&start.policy_action(), 1)?;
        let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
        let checks = requests
            .iter()
            .map(|(method, path)| {
                let status = match (method, *path) {
                    (LoopbackMethod::Get, "/redirect") => 302,
                    (LoopbackMethod::Get, "/missing") => 404,
                    (LoopbackMethod::Get, _) => 200,
                    _ => 405,
                };
                LoopbackHttpCheck::new(
                    LoopbackProbe::new(*method, (*path).to_owned())?,
                    status,
                    LoopbackExpectedBody::AnyBounded,
                    LoopbackHeaderChecks::default(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let recipe = PreparedLoopbackVerification::new(
            start,
            FileRevision::new(path.clone(), hash(SERVER.as_bytes())),
            SnapshotId::from_bytes([7; 32]),
            LoopbackHttpRecipe::new(path, LoopbackStart::Explicit, port, checks)?,
        )?;
        let decisions = (0..requests.len())
            .map(|index| {
                let action = recipe.probe_policy_action(index).ok_or("missing probe")?;
                approve(&action, u8::try_from(index)? + 2)
            })
            .collect::<TestResult<Vec<_>>>()?;
        Ok(AuthorizedLoopbackVerification::new(
            recipe,
            &start_decision,
            &decisions,
        )?)
    }

    fn start_spec(
        &self,
        port: NonZeroU16,
        timeout_millis: u64,
        argv: &[&str],
    ) -> TestResult<ProcessSpec> {
        Ok(ProcessSpec::new(
            ProcessSpecSchemaVersion::V2,
            RUN,
            self.project.worktree().id(),
            ProcessExecutable::try_from_string("python".to_owned())?,
            argv.iter()
                .map(|arg| ProcessArgument::try_from_string((*arg).to_owned()))
                .collect::<Result<Vec<_>, _>>()?,
            WorkspaceDirectory::Root,
            child_environment()?,
            ProcessTimeout::from_millis(timeout_millis)?,
            ProcessOutputLimit::new(512)?,
            ProcessOutputLimit::new(512)?,
            ProcessExecutionMode::Open,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([21; 32])),
            ProcessNetworkScope::Requested(PreparedLoopbackVerification::network_target(port)),
        )?)
    }
}

fn child_environment() -> TestResult<Vec<ProcessEnvironmentVariable>> {
    if cfg!(windows) {
        Ok(vec![ProcessEnvironmentVariable::try_from_string(
            "SYSTEMROOT".to_owned(),
        )?])
    } else {
        Ok(Vec::new())
    }
}

fn approve(action: &PolicyAction, index: u8) -> TestResult<PolicyDecision> {
    let at = AgentRunTimestamp::from_unix_millis(100)?;
    let id = decision_id(index);
    let request = ApprovalRequest::new(
        ApprovalRequestId::from_bytes([index; 32]),
        RUN,
        action,
        at,
        AgentRunTimestamp::from_unix_millis(200)?,
    )?;
    let mut grant = ApprovalGrant::grant(ApprovalId::from_bytes([index; 32]), &request, at)?;
    grant.consume(id, RUN, action, at)?;
    Ok(PolicyDecision::approved(
        id,
        RUN,
        action,
        &grant,
        PolicyEvaluationTiming::new(at, at)?,
    )?)
}

fn decision_id(index: u8) -> PolicyDecisionId {
    PolicyDecisionId::from_bytes([index; 32])
}

fn hash(bytes: &[u8]) -> ContentHash {
    ContentHash::from_bytes(*blake3::hash(bytes).as_bytes())
}

fn unused_port() -> TestResult<NonZeroU16> {
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    NonZeroU16::new(socket.local_addr()?.port()).ok_or_else(|| "invalid port".into())
}

fn assert_stopped(port: NonZeroU16) {
    let endpoint = SocketAddr::from((Ipv4Addr::LOCALHOST, port.get()));
    assert!(TcpStream::connect_timeout(&endpoint, Duration::from_millis(100)).is_err());
}
