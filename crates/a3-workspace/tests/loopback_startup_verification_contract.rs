//! Real compound startup observations, exact approvals and owned cleanup.

#![cfg(any(windows, target_os = "linux"))]

mod support;

use a3_application::{
    AuthorizedLoopbackStartupCoverage, LoopbackStartupCleanupProof,
    LoopbackStartupVerificationOutcome, LoopbackVerificationFailure, LoopbackVerificationRunner,
    PreparedLoopbackStartupCoverage, ProcessRunControl,
};
use a3_domain::{
    AgentRunId, AgentRunTimestamp, ApprovalGrant, ApprovalId, ApprovalRequest, ApprovalRequestId,
    CanonicalDirectory, ContentHash, FileRevision, GitHead, GitReferenceName, LoopbackExpectedBody,
    LoopbackHeaderChecks, LoopbackHttpCheck, LoopbackHttpRecipe, LoopbackMethod, LoopbackProbe,
    LoopbackProcessSettings, LoopbackStart, PolicyAction, PolicyDecision, PolicyDecisionId,
    PolicyEvaluationTiming, ProcessEnvironmentVariable, ProcessOutputLimit, ProcessTimeout,
    ProjectIdentity, RepositoryId, RepositoryIdentity, RepositoryPath, SnapshotId, TaskStepId,
    WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use a3_workspace::{ProcessHostEnvironment, WorkspaceLoopbackVerificationRunner};
use std::error::Error;
use std::fs;
use std::net::{Ipv4Addr, TcpListener};
use std::num::NonZeroU16;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use support::TempDirectory;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
const RUN: AgentRunId = AgentRunId::from_bytes([20; 32]);
static STARTUP_TEST_LOCK: Mutex<()> = Mutex::new(());

const SERVER: &str = r#"from argparse import ArgumentParser
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from time import sleep
import sys

with Path(__file__).with_name('starts.log').open('a', encoding='utf-8') as audit:
    audit.write((' '.join(sys.argv[1:]) or '<default>') + '\n')

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        body = b'Healthy\n' if self.path == '/health' else b''
        self.send_response(200 if self.path == '/health' else 404)
        self.send_header('Content-Type', 'text/plain; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def reject(self):
        self.send_response(405)
        self.send_header('Allow', 'GET')
        self.send_header('Content-Length', '0')
        self.end_headers()

    do_POST = do_PUT = do_DELETE = do_PATCH = reject

parser = ArgumentParser()
parser.add_argument('--host', default='127.0.0.1')
parser.add_argument('--port', default=8765, type=int)
args = parser.parse_args()
try:
    server = HTTPServer((args.host, args.port), Handler)
except OSError:
    BIND_FAILURE
with server:
    server.serve_forever()
"#;

#[test]
fn compound_startup_observes_four_starts_and_releases_both_endpoints() -> TestResult {
    let _guard = lock_tests()?;
    let fixture = Fixture::new("raise SystemExit(1)")?;
    let port = unused_port()?;
    let prepared = fixture.prepared(port, 12_000)?;
    let actions = prepared.policy_actions();
    let decisions = approvals(&actions)?;
    let expected_ids = decisions.iter().map(PolicyDecision::id).collect::<Vec<_>>();
    let authorized = AuthorizedLoopbackStartupCoverage::new(prepared, &decisions)?;
    assert!(fixture.runner.supports_startup_coverage());
    let outcome = futures::executor::block_on(fixture.runner.verify_startup(
        &fixture.project,
        authorized,
        &Control::default(),
    ));
    let LoopbackStartupVerificationOutcome::Stopped { cleanup, result } = outcome else {
        return Err(format!("expected compound cleanup, got {outcome:?}").into());
    };
    assert_eq!(cleanup, expected_cleanup());
    let report = result?;
    assert_eq!(report.decisions.ordered(), expected_ids);
    assert_eq!(report.held_exit.get(), 1);
    assert_eq!(report.invalid_exit.get(), 2);
    assert_eq!(
        report
            .explicit_probes
            .iter()
            .map(|probe| probe.status())
            .collect::<Vec<_>>(),
        [200, 404, 405, 405, 405, 405]
    );
    assert_eq!(report.default_probe.status(), 200);
    assert_eq!(report.default_probe.body_hash(), hash(b"Healthy\n"));
    assert!(report.default_probe.headers().plain_utf8);
    let explicit = format!("--host 127.0.0.1 --port {port}");
    assert_eq!(
        fixture.start_log()?,
        [
            explicit.clone(),
            explicit,
            "<default>".to_owned(),
            "--port invalid".to_owned()
        ]
    );
    assert_released(port)?;
    assert_released(NonZeroU16::try_from(8765)?)?;
    Ok(())
}

#[test]
fn compound_startup_reuses_the_same_port_after_mismatch_correction_and_complete_cleanup_twice()
-> TestResult {
    let _guard = lock_tests()?;
    let mut fixture = Fixture::new("raise SystemExit(1)")?;
    let correct = fixture.source.clone();
    let wrong = correct.replace("b'Healthy\\n'", "b'Wrong!!\\n'");
    assert_ne!(wrong, correct);
    let port = unused_port()?;
    let explicit = format!("--host 127.0.0.1 --port {port}");
    for cycle in 0_u8..2 {
        // Every rewrite follows the preceding observed complete owned cleanup.
        fixture.source = wrong.clone();
        fs::write(fixture.temp.path().join("server.py"), &fixture.source)?;
        let prepared = fixture.prepared(port, 12_000)?;
        let old_hash = prepared.script().content_hash();
        let wrong_decisions = approvals_with_seed(&prepared.policy_actions(), cycle * 32)?;
        let outcome = futures::executor::block_on(fixture.runner.verify_startup(
            &fixture.project,
            AuthorizedLoopbackStartupCoverage::new(prepared, &wrong_decisions)?,
            &Control::default(),
        ));
        let LoopbackStartupVerificationOutcome::Mismatched { cleanup, mismatch } = outcome else {
            return Err(format!("real mismatch lost in cycle {cycle}: {outcome:?}").into());
        };
        assert_eq!(cleanup, expected_cleanup());
        assert_eq!(
            mismatch.phase(),
            a3_domain::LoopbackStartupPhase::FreeExplicit
        );
        assert_eq!(mismatch.check_index(), 0);
        assert_eq!(mismatch.observation().status(), 200);
        assert_eq!(mismatch.observation().body_hash(), hash(b"Wrong!!\n"));
        assert_eq!(mismatch.observation().body_bytes(), 8);

        fixture.source = correct.clone();
        fs::write(fixture.temp.path().join("server.py"), &fixture.source)?;
        let prepared = fixture.prepared(port, 12_000)?;
        assert_ne!(prepared.script().content_hash(), old_hash);
        assert!(matches!(
            AuthorizedLoopbackStartupCoverage::new(prepared, &wrong_decisions),
            Err(LoopbackVerificationFailure::Denied)
        ));
        let prepared = fixture.prepared(port, 12_000)?;
        let decisions = approvals_with_seed(&prepared.policy_actions(), cycle * 32 + 16)?;
        let expected_ids = decisions.iter().map(PolicyDecision::id).collect::<Vec<_>>();
        assert!(
            decisions
                .iter()
                .all(|fresh| wrong_decisions.iter().all(|old| old.id() != fresh.id()))
        );
        let outcome = futures::executor::block_on(fixture.runner.verify_startup(
            &fixture.project,
            AuthorizedLoopbackStartupCoverage::new(prepared, &decisions)?,
            &Control::default(),
        ));
        let LoopbackStartupVerificationOutcome::Stopped { cleanup, result } = outcome else {
            return Err(
                format!("same-port corrected recipe failed in cycle {cycle}: {outcome:?}").into(),
            );
        };
        assert_eq!(cleanup, expected_cleanup());
        let report = result?;
        assert_eq!(report.decisions.ordered(), expected_ids);
        assert_eq!((report.held_exit.get(), report.invalid_exit.get()), (1, 2));
        assert_eq!(
            report
                .explicit_probes
                .iter()
                .map(|probe| probe.status())
                .collect::<Vec<_>>(),
            [200, 404, 405, 405, 405, 405]
        );
        assert_eq!(report.explicit_probes[0].body_hash(), hash(b"Healthy\n"));
        assert_eq!(report.explicit_probes[0].body_bytes(), 8);
        assert_eq!(report.default_probe.body_hash(), hash(b"Healthy\n"));
        assert!(report.default_probe.headers().plain_utf8);
        assert_released(port)?;
        assert_released(NonZeroU16::try_from(8765)?)?;
        let cycle_log = [
            explicit.clone(),
            explicit.clone(),
            explicit.clone(),
            explicit.clone(),
            "<default>".to_owned(),
            "--port invalid".to_owned(),
        ];
        let expected = (0..=cycle)
            .flat_map(|_| cycle_log.iter().cloned())
            .collect::<Vec<_>>();
        assert_eq!(fixture.start_log()?, expected);
    }
    Ok(())
}

#[test]
fn compound_startup_truncated_declared_body_cannot_create_report_even_when_payload_matches()
-> TestResult {
    let _guard = lock_tests()?;
    let mut fixture = Fixture::new("raise SystemExit(1)")?;
    let original = fixture.source.clone();
    fixture.source = original.replace(
        "self.send_header('Content-Length', str(len(body)))",
        "self.send_header('Content-Length', str(len(body) + 1))",
    );
    assert_ne!(fixture.source, original);
    fs::write(fixture.temp.path().join("server.py"), &fixture.source)?;
    let port = unused_port()?;
    let prepared = fixture.prepared(port, 12_000)?;
    assert!(matches!(prepared.recipe().checks()[0].body(),
        LoopbackExpectedBody::Exact { hash: expected, bytes: 8 } if expected == hash(b"Healthy\n")));
    // The unmodified writer emits all eight expected Healthy-LF bytes, but the
    // response declares nine. Framing must fail before any successful observation.
    let decisions = approvals_with_seed(&prepared.policy_actions(), 64)?;
    let outcome = futures::executor::block_on(fixture.runner.verify_startup(
        &fixture.project,
        AuthorizedLoopbackStartupCoverage::new(prepared, &decisions)?,
        &Control::default(),
    ));
    assert_eq!(
        outcome,
        LoopbackStartupVerificationOutcome::Stopped {
            cleanup: expected_cleanup(),
            result: Err(LoopbackVerificationFailure::InvalidResponse),
        }
    );
    let explicit = format!("--host 127.0.0.1 --port {port}");
    assert_eq!(fixture.start_log()?, [explicit.clone(), explicit]);
    assert_released(port)?;
    assert_released(NonZeroU16::try_from(8765)?)?;
    Ok(())
}

#[test]
fn compound_startup_foreign_port_stale_source_and_swapped_phase_grants_never_start() -> TestResult {
    let _guard = lock_tests()?;
    let fixture = Fixture::new("raise SystemExit(1)")?;
    let foreign = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = NonZeroU16::try_from(foreign.local_addr()?.port())?;
    let outcome = futures::executor::block_on(fixture.runner.verify_startup(
        &fixture.project,
        fixture.authorized(port, 12_000)?,
        &Control::default(),
    ));
    assert_eq!(
        outcome,
        LoopbackStartupVerificationOutcome::NotStarted(LoopbackVerificationFailure::Occupied)
    );
    foreign.set_nonblocking(true)?;
    assert!(
        matches!(foreign.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    assert!(fixture.start_log()?.is_empty());
    drop(foreign);

    let prepared = fixture.prepared(port, 12_000)?;
    let mut decisions = approvals(&prepared.policy_actions())?;
    decisions.swap(1, 2);
    assert!(matches!(
        AuthorizedLoopbackStartupCoverage::new(prepared, &decisions),
        Err(LoopbackVerificationFailure::Denied)
    ));
    assert!(fixture.start_log()?.is_empty());

    let authorized = fixture.authorized(port, 12_000)?;
    fs::write(
        fixture.temp.path().join("server.py"),
        b"raise RuntimeError()\n",
    )?;
    assert_eq!(
        futures::executor::block_on(fixture.runner.verify_startup(
            &fixture.project,
            authorized,
            &Control::default(),
        )),
        LoopbackStartupVerificationOutcome::NotStarted(LoopbackVerificationFailure::StaleScript)
    );
    assert!(fixture.start_log()?.is_empty());
    Ok(())
}

#[test]
fn compound_startup_zero_exit_has_no_full_report_and_stops_before_free_start() -> TestResult {
    let _guard = lock_tests()?;
    let fixture = Fixture::new("raise SystemExit(0)")?;
    let port = unused_port()?;
    let outcome = futures::executor::block_on(fixture.runner.verify_startup(
        &fixture.project,
        fixture.authorized(port, 12_000)?,
        &Control::default(),
    ));
    assert_eq!(
        outcome,
        LoopbackStartupVerificationOutcome::Stopped {
            cleanup: expected_cleanup(),
            result: Err(LoopbackVerificationFailure::Exited),
        }
    );
    assert_eq!(fixture.start_log()?.len(), 1);
    assert_released(port)?;
    Ok(())
}

#[test]
fn compound_startup_checks_revision_after_failed_and_successful_held_exits() -> TestResult {
    let _guard = lock_tests()?;
    for code in [0, 1] {
        let fixture = Fixture::new(&format!(
            "Path(__file__).write_text('raise RuntimeError()\\n', encoding='utf-8'); raise SystemExit({code})"
        ))?;
        let port = unused_port()?;
        let outcome = futures::executor::block_on(fixture.runner.verify_startup(
            &fixture.project,
            fixture.authorized(port, 12_000)?,
            &Control::default(),
        ));
        assert_eq!(
            outcome,
            LoopbackStartupVerificationOutcome::Stopped {
                cleanup: expected_cleanup(),
                result: Err(LoopbackVerificationFailure::StaleScript),
            }
        );
        assert_eq!(fixture.start_log()?.len(), 1);
        assert_released(port)?;
    }
    Ok(())
}

#[test]
fn compound_startup_wrong_explicit_response_prevents_later_starts() -> TestResult {
    let _guard = lock_tests()?;
    let mut fixture = Fixture::new("raise SystemExit(1)")?;
    fixture.source = fixture.source.replace("b'Healthy\\n'", "b'Wrong!!\\n'");
    fs::write(fixture.temp.path().join("server.py"), &fixture.source)?;
    let port = unused_port()?;
    let outcome = futures::executor::block_on(fixture.runner.verify_startup(
        &fixture.project,
        fixture.authorized(port, 12_000)?,
        &Control::default(),
    ));
    let LoopbackStartupVerificationOutcome::Mismatched { cleanup, mismatch } = outcome else {
        return Err(format!("actual wrong response lost: {outcome:?}").into());
    };
    assert_eq!(cleanup, expected_cleanup());
    assert_eq!(
        mismatch.phase(),
        a3_domain::LoopbackStartupPhase::FreeExplicit
    );
    assert_eq!(mismatch.check_index(), 0);
    assert_eq!(mismatch.observation().status(), 200);
    assert_eq!(mismatch.observation().body_hash(), hash(b"Wrong!!\n"));
    assert_eq!(mismatch.observation().body_bytes(), 8);
    let explicit = format!("--host 127.0.0.1 --port {port}");
    assert_eq!(fixture.start_log()?, [explicit.clone(), explicit]);
    assert_released(port)?;
    assert_released(NonZeroU16::try_from(8765)?)?;
    Ok(())
}

#[test]
fn compound_startup_does_not_renew_the_deadline_between_phases() -> TestResult {
    let _guard = lock_tests()?;
    let mut fixture = Fixture::new("raise SystemExit(1)")?;
    fixture.source = fixture.source.replace(
        "parser = ArgumentParser()",
        "sleep(1.2)\nparser = ArgumentParser()",
    );
    fs::write(fixture.temp.path().join("server.py"), &fixture.source)?;
    let port = unused_port()?;
    let outcome = futures::executor::block_on(fixture.runner.verify_startup(
        &fixture.project,
        fixture.authorized(port, 6_000)?,
        &Control::default(),
    ));
    assert_eq!(
        outcome,
        LoopbackStartupVerificationOutcome::Stopped {
            cleanup: expected_cleanup(),
            result: Err(LoopbackVerificationFailure::TimedOut),
        }
    );
    let starts = fixture.start_log()?;
    assert!(
        (2..=4).contains(&starts.len()),
        "observed starts: {starts:?}"
    );
    assert_released(port)?;
    assert_released(NonZeroU16::try_from(8765)?)?;
    Ok(())
}

#[test]
fn compound_startup_default_mismatch_retains_original_check_and_cleanup() -> TestResult {
    let _guard = lock_tests()?;
    let mut fixture = Fixture::new("raise SystemExit(1)")?;
    fixture.source = fixture.source.replace(
        "body = b'Healthy\\n' if self.path == '/health' else b''",
        "body = b'Wrong!!\\n' if self.server.server_port == 8765 else (b'Healthy\\n' if self.path == '/health' else b'')");
    fs::write(fixture.temp.path().join("server.py"), &fixture.source)?;
    let port = unused_port()?;
    let outcome = futures::executor::block_on(fixture.runner.verify_startup(
        &fixture.project,
        fixture.authorized(port, 12_000)?,
        &Control::default(),
    ));
    let LoopbackStartupVerificationOutcome::Mismatched { cleanup, mismatch } = outcome else {
        return Err(format!("default response lost: {outcome:?}").into());
    };
    assert_eq!(cleanup, expected_cleanup());
    assert_eq!(mismatch.phase(), a3_domain::LoopbackStartupPhase::Default);
    assert_eq!(mismatch.check_index(), 0);
    assert_eq!(mismatch.observation().body_hash(), hash(b"Wrong!!\n"));
    assert_eq!(fixture.start_log()?.len(), 3);
    assert_released(port)?;
    assert_released(NonZeroU16::try_from(8765)?)?;
    Ok(())
}

#[test]
fn compound_startup_changed_script_overrides_a_complete_wrong_response() -> TestResult {
    let _guard = lock_tests()?;
    let mut fixture = Fixture::new("raise SystemExit(1)")?;
    fixture.source = fixture.source.replace("b'Healthy\\n'", "b'Wrong!!\\n'").replace(
        "self.wfile.write(body)",
        "Path(__file__).write_text(Path(__file__).read_text(encoding='utf-8') + '# changed\\n', encoding='utf-8')\n        self.wfile.write(body)");
    fs::write(fixture.temp.path().join("server.py"), &fixture.source)?;
    let port = unused_port()?;
    let outcome = futures::executor::block_on(fixture.runner.verify_startup(
        &fixture.project,
        fixture.authorized(port, 12_000)?,
        &Control::default(),
    ));
    assert_eq!(
        outcome,
        LoopbackStartupVerificationOutcome::Stopped {
            cleanup: expected_cleanup(),
            result: Err(LoopbackVerificationFailure::StaleScript),
        }
    );
    assert_released(port)?;
    assert_released(NonZeroU16::try_from(8765)?)?;
    Ok(())
}

#[test]
fn compound_startup_timeout_and_cancellation_close_the_held_endpoint() -> TestResult {
    let _guard = lock_tests()?;
    for cancel in [false, true] {
        let fixture =
            Fixture::new("Path('occupied-entered').touch(); sleep(30); raise SystemExit(1)")?;
        let port = unused_port()?;
        let control = Control::default();
        let authorized = fixture.authorized(port, if cancel { 12_000 } else { 4_000 })?;
        let outcome = if cancel {
            thread::scope(|scope| {
                scope.spawn(|| {
                    let end = Instant::now() + Duration::from_secs(6);
                    while !fixture.temp.path().join("occupied-entered").exists()
                        && Instant::now() < end
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    control.0.store(true, Ordering::Relaxed);
                });
                futures::executor::block_on(fixture.runner.verify_startup(
                    &fixture.project,
                    authorized,
                    &control,
                ))
            })
        } else {
            futures::executor::block_on(fixture.runner.verify_startup(
                &fixture.project,
                authorized,
                &control,
            ))
        };
        assert_eq!(
            outcome,
            LoopbackStartupVerificationOutcome::Stopped {
                cleanup: expected_cleanup(),
                result: Err(if cancel {
                    LoopbackVerificationFailure::Cancelled
                } else {
                    LoopbackVerificationFailure::TimedOut
                }),
            }
        );
        assert_eq!(fixture.start_log()?.len(), 1);
        assert_released(port)?;
    }
    Ok(())
}

fn lock_tests() -> TestResult<std::sync::MutexGuard<'static, ()>> {
    Ok(STARTUP_TEST_LOCK
        .lock()
        .map_err(|_| std::io::Error::other("startup test lock poisoned"))?)
}

const fn expected_cleanup() -> LoopbackStartupCleanupProof {
    if cfg!(windows) {
        LoopbackStartupCleanupProof::WindowsJobsAndEndpoints
    } else {
        LoopbackStartupCleanupProof::ProcessGroupsAndEndpoints
    }
}

fn unused_port() -> TestResult<NonZeroU16> {
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    Ok(NonZeroU16::try_from(socket.local_addr()?.port())?)
}

fn assert_released(port: NonZeroU16) -> TestResult {
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, port.get()))?;
    assert_eq!(socket.local_addr()?.port(), port.get());
    Ok(())
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
    source: String,
    project: ProjectIdentity,
    runner: WorkspaceLoopbackVerificationRunner,
}

impl Fixture {
    fn new(bind_failure: &str) -> TestResult<Self> {
        let temp = TempDirectory::new()?;
        let source = SERVER.replace("BIND_FAILURE", bind_failure);
        fs::write(temp.path().join("server.py"), &source)?;
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
        let mut environment = vec![ProcessEnvironmentVariable::try_from_string(
            "PATH".to_owned(),
        )?];
        if cfg!(windows) {
            environment.push(ProcessEnvironmentVariable::try_from_string(
                "SYSTEMROOT".to_owned(),
            )?);
        }
        Ok(Self {
            temp,
            source,
            project,
            runner: WorkspaceLoopbackVerificationRunner::new(ProcessHostEnvironment::capture(
                environment,
            )?),
        })
    }

    fn prepared(
        &self,
        port: NonZeroU16,
        timeout: u64,
    ) -> TestResult<PreparedLoopbackStartupCoverage> {
        let path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
        let mut checks = vec![
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/health".to_owned())?,
                200,
                LoopbackExpectedBody::Exact {
                    hash: hash(b"Healthy\n"),
                    bytes: 8,
                },
                LoopbackHeaderChecks {
                    plain_utf8: true,
                    allow_get: false,
                },
            )?,
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/missing".to_owned())?,
                404,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks::default(),
            )?,
        ];
        for method in [
            LoopbackMethod::Post,
            LoopbackMethod::Put,
            LoopbackMethod::Delete,
            LoopbackMethod::Patch,
        ] {
            checks.push(LoopbackHttpCheck::new(
                LoopbackProbe::new(method, "/health".to_owned())?,
                405,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks {
                    plain_utf8: false,
                    allow_get: true,
                },
            )?);
        }
        let recipe = LoopbackHttpRecipe::new(path.clone(), LoopbackStart::Explicit, port, checks)?
            .with_startup_coverage(0)?;
        let limit = ProcessOutputLimit::new(512)?;
        Ok(PreparedLoopbackStartupCoverage::new(
            RUN,
            self.project.worktree().id(),
            TaskStepId::from_bytes([21; 32]),
            SnapshotId::from_bytes([7; 32]),
            FileRevision::new(path, hash(self.source.as_bytes())),
            recipe,
            LoopbackProcessSettings {
                timeout: ProcessTimeout::from_millis(timeout)?,
                stdout_limit: limit,
                stderr_limit: limit,
                system_root: cfg!(windows),
            },
        )?)
    }

    fn authorized(
        &self,
        port: NonZeroU16,
        timeout: u64,
    ) -> TestResult<AuthorizedLoopbackStartupCoverage> {
        let prepared = self.prepared(port, timeout)?;
        let decisions = approvals(&prepared.policy_actions())?;
        Ok(AuthorizedLoopbackStartupCoverage::new(
            prepared, &decisions,
        )?)
    }

    fn start_log(&self) -> TestResult<Vec<String>> {
        match fs::read_to_string(self.temp.path().join("starts.log")) {
            Ok(text) => Ok(text.lines().map(str::to_owned).collect()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }
}

fn approvals(actions: &[PolicyAction]) -> TestResult<Vec<PolicyDecision>> {
    approvals_with_seed(actions, 0)
}

fn approvals_with_seed(
    actions: &[PolicyAction],
    seed_offset: u8,
) -> TestResult<Vec<PolicyDecision>> {
    actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let seed = seed_offset
                .checked_add(u8::try_from(index + 1)?)
                .ok_or("test approval seed overflow")?;
            let at = AgentRunTimestamp::from_unix_millis(100)?;
            let id = PolicyDecisionId::from_bytes([seed; 32]);
            let request = ApprovalRequest::new(
                ApprovalRequestId::from_bytes([seed; 32]),
                RUN,
                action,
                at,
                AgentRunTimestamp::from_unix_millis(200)?,
            )?;
            let mut grant = ApprovalGrant::grant(ApprovalId::from_bytes([seed; 32]), &request, at)?;
            grant.consume(id, RUN, action, at)?;
            Ok(PolicyDecision::approved(
                id,
                RUN,
                action,
                &grant,
                PolicyEvaluationTiming::new(at, at)?,
            )?)
        })
        .collect()
}

fn hash(bytes: &[u8]) -> ContentHash {
    ContentHash::from_bytes(*blake3::hash(bytes).as_bytes())
}
