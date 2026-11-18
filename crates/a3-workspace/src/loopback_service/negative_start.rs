//! Linux candidate prerequisites for observing rejected server starts.

use super::{
    CLEANUP_RESERVE, Failure, MAX_RECIPE_TIME, POLL, check_revision, group_stopped, remaining,
    time_until,
};
use crate::{
    ProcessHostEnvironment, process_launch::prepare_command, process_runner::spawn_process_group,
};
use a3_application::{AuthorizedProcessSpec, ProcessRunControl};
use a3_domain::{
    FileRevision, LoopbackHttpRecipe, MachineProcessEffect, PolicyDecisionId, PolicyResourceId,
    ProcessArgument, ProcessExecutionMode, ProcessNetworkScope, ProcessPlanBinding, ProcessSpec,
    ProcessSpecSchemaVersion, ProjectIdentity, WorkspaceDirectory, loopback_script_path,
};
use command_group::GroupChild;
use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
use std::{
    io::ErrorKind,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    num::NonZeroU16,
    path::Path,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};
use tokio::net::TcpSocket;

const SCRIPT: &str = "server.py";
const ARGV: [&str; 4] = ["-B", SCRIPT, "--port", "invalid"];

#[derive(Debug, PartialEq, Eq)]
enum StartOutcome {
    NotStarted(Failure),
    Stopped(Result<ObservedExit, Failure>),
    UnknownCleanup,
}

#[derive(Clone, Copy)]
enum StartCase {
    InvalidPort,
    OccupiedExplicit(NonZeroU16),
    FreeExplicit(NonZeroU16),
}

#[derive(Debug, PartialEq, Eq)]
struct ObservedExit {
    specification_id: PolicyResourceId,
    decision_id: PolicyDecisionId,
    code: i32,
}

fn observe_start(
    project: &ProjectIdentity,
    script: &FileRevision,
    authorized: AuthorizedProcessSpec,
    environment: &ProcessHostEnvironment,
    control: &dyn ProcessRunControl,
    case: StartCase,
) -> StartOutcome {
    let (spec, decision_id) = authorized.into_parts();
    let timeout = Duration::from_millis(spec.timeout().as_millis());
    if !exact_start(&spec, script, case) || timeout <= CLEANUP_RESERVE || timeout > MAX_RECIPE_TIME
    {
        return StartOutcome::NotStarted(Failure::Denied);
    }
    let end = Instant::now() + timeout;
    let observe_until = end - CLEANUP_RESERVE;
    let prepared = (|| {
        remaining(observe_until, control)?;
        let mut command =
            prepare_command(project, &spec, environment).map_err(|_| Failure::Denied)?;
        if Path::new(command.get_program()).starts_with(project.worktree().root().as_path()) {
            return Err(Failure::Denied);
        }
        // This slice observes exit only. There are no HTTP probes or output readers.
        command.stdout(Stdio::null()).stderr(Stdio::null());
        check_revision(project, script, control)?;
        let reservation = match case {
            StartCase::OccupiedExplicit(port) => Some(reserve_endpoint(port)?),
            StartCase::InvalidPort | StartCase::FreeExplicit(_) => None,
        };
        remaining(observe_until, control)?;
        Ok((command, reservation))
    })();
    let (mut command, reservation) = match prepared {
        Ok(command) => command,
        Err(failure) => return StartOutcome::NotStarted(failure),
    };
    let Ok(mut child) = spawn_process_group(&mut command) else {
        // A launch error after OS process creation is not proof of an absent effect.
        return StartOutcome::UnknownCleanup;
    };

    let Some(observed) = observe_natural_exit(child.id(), observe_until, control).transpose()
    else {
        // The PID is no longer waitable. Do not signal a numeric group whose owner is lost.
        return StartOutcome::UnknownCleanup;
    };
    if !stop_group(&mut child, end) {
        return StartOutcome::UnknownCleanup;
    }
    // Keep the exact endpoint bound until the entire owned group has stopped.
    drop(reservation);
    StartOutcome::Stopped(observed.and_then(|code| {
        check_revision(project, script, control)?;
        remaining(end, control)?;
        if matches!(case, StartCase::OccupiedExplicit(_)) && code == 0 {
            return Err(Failure::Exited);
        }
        Ok(ObservedExit {
            specification_id: spec.specification_id(),
            decision_id,
            code,
        })
    }))
}

fn exact_start(spec: &ProcessSpec, script: &FileRevision, case: StartCase) -> bool {
    let Some(script) = loopback_script_path(script.path()) else {
        return false;
    };
    let arguments = spec.arguments().iter().map(ProcessArgument::as_str);
    let (exact_argv, network) = match case {
        StartCase::InvalidPort => (
            arguments.eq(["-B", script, "--port", "invalid"]),
            ProcessNetworkScope::Denied,
        ),
        StartCase::OccupiedExplicit(port) | StartCase::FreeExplicit(port) => (
            arguments.eq([
                "-B",
                script,
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
            ]),
            ProcessNetworkScope::Requested(LoopbackHttpRecipe::network_target(port)),
        ),
    };
    spec.version() == ProcessSpecSchemaVersion::V2
        && spec.executable().as_str() == "python"
        && exact_argv
        && *spec.working_directory() == WorkspaceDirectory::Root
        && spec.environment_allowlist().is_empty()
        && spec.execution_mode() == ProcessExecutionMode::Open
        && matches!(spec.plan_binding(), ProcessPlanBinding::Validated(_))
        && spec.network() == network
        && spec.machine_effect() == Some(MachineProcessEffect::Unknown)
}

fn reserve_endpoint(port: NonZeroU16) -> Result<TcpSocket, Failure> {
    let socket = TcpSocket::new_v4().map_err(|_| Failure::Unavailable)?;
    let endpoint = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port.get()));
    // The socket is bound only. Neither reuse option nor listen is enabled.
    socket.bind(endpoint).map_err(|error| match error.kind() {
        ErrorKind::AddrInUse => Failure::Occupied,
        _ => Failure::Unavailable,
    })?;
    Ok(socket)
}

fn observe_natural_exit(
    child_id: u32,
    end: Instant,
    control: &dyn ProcessRunControl,
) -> Result<Option<i32>, Failure> {
    let pid = i32::try_from(child_id)
        .ok()
        .and_then(Pid::from_raw)
        .ok_or(Failure::Unavailable)?;
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
    loop {
        let time = remaining(end, control)?;
        // Keep the leader waitable so that its PID still anchors the group at kill time.
        match waitid(WaitId::Pid(pid), options) {
            Ok(Some(status)) => {
                remaining(end, control)?;
                return status.exit_status().map(Some).ok_or(Failure::Exited);
            }
            Ok(None) | Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::CHILD) => return Ok(None),
            Err(_) => return Err(Failure::Unavailable),
        }
        if control.wait_cancelled_timeout(time.min(POLL)) {
            return Err(Failure::Cancelled);
        }
    }
}

fn stop_group(child: &mut GroupChild, end: Instant) -> bool {
    if let Err(error) = child.kill()
        && error.raw_os_error() != Some(rustix::io::Errno::SRCH.raw_os_error())
    {
        return false;
    }
    let mut reaped = false;
    while let Some(time) = time_until(end) {
        if !reaped {
            match child.try_wait() {
                Ok(status) => reaped = status.is_some(),
                Err(_) => return false,
            }
        }
        if reaped {
            match group_stopped(child.id()) {
                Some(true) => return time_until(end).is_some(),
                Some(false) => {}
                None => return false,
            }
        }
        thread::sleep(time.min(POLL));
    }
    false
}

#[path = "../../tests/support/mod.rs"]
mod support;

#[cfg(test)]
mod tests {
    use super::{support::TempDirectory, *};
    use a3_domain::{
        AgentRunId, AgentRunTimestamp, ApprovalGrant, ApprovalId, ApprovalRequest,
        ApprovalRequestId, CanonicalDirectory, ContentHash, GitHead, GitReferenceName,
        PolicyDecision, PolicyEvaluationTiming, ProcessEnvironmentVariable, ProcessExecutable,
        ProcessOutputLimit, ProcessTimeout, RepositoryId, RepositoryIdentity, RepositoryPath,
        TaskStepId, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
    };
    use std::{error::Error, fs, path::PathBuf};

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;

    #[derive(Debug)]
    struct Control;

    impl ProcessRunControl for Control {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn wait_cancelled_timeout(&self, time: Duration) -> bool {
            thread::sleep(time);
            false
        }
    }

    #[derive(Debug)]
    struct CancelAfterEntry(PathBuf);

    impl ProcessRunControl for CancelAfterEntry {
        fn is_cancelled(&self) -> bool {
            self.0.is_file()
        }

        fn wait_cancelled_timeout(&self, time: Duration) -> bool {
            thread::sleep(time);
            self.is_cancelled()
        }
    }

    struct Fixture {
        temp: TempDirectory,
        project: ProjectIdentity,
        script: FileRevision,
        environment: ProcessHostEnvironment,
    }

    impl Fixture {
        fn new(source: &str) -> TestResult<Self> {
            let temp = TempDirectory::new()?;
            fs::write(temp.path().join(SCRIPT), source)?;
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
            let script = FileRevision::new(
                RepositoryPath::try_from_bytes(SCRIPT.as_bytes().to_vec())?,
                ContentHash::from_bytes(*blake3::hash(source.as_bytes()).as_bytes()),
            );
            let environment = ProcessHostEnvironment::capture(vec![
                ProcessEnvironmentVariable::try_from_string("PATH".to_owned())?,
            ])?;
            Ok(Self {
                temp,
                project,
                script,
                environment,
            })
        }

        fn authorize(
            &self,
            arguments: [&str; 4],
        ) -> TestResult<(AuthorizedProcessSpec, PolicyDecisionId)> {
            self.authorize_process(arguments, ProcessNetworkScope::Denied, 7)
        }

        fn authorize_explicit(
            &self,
            port: NonZeroU16,
            approval_seed: u8,
        ) -> TestResult<(AuthorizedProcessSpec, PolicyDecisionId)> {
            self.authorize_process(
                [
                    "-B",
                    SCRIPT,
                    "--host",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                ],
                ProcessNetworkScope::Requested(LoopbackHttpRecipe::network_target(port)),
                approval_seed,
            )
        }

        fn authorize_process<const N: usize>(
            &self,
            arguments: [&str; N],
            network: ProcessNetworkScope,
            approval_seed: u8,
        ) -> TestResult<(AuthorizedProcessSpec, PolicyDecisionId)> {
            let run = AgentRunId::from_bytes([4; 32]);
            let spec = ProcessSpec::new(
                ProcessSpecSchemaVersion::V2,
                run,
                self.project.worktree().id(),
                ProcessExecutable::try_from_string("python".to_owned())?,
                arguments
                    .into_iter()
                    .map(|value| ProcessArgument::try_from_string(value.to_owned()))
                    .collect::<Result<Vec<_>, _>>()?,
                WorkspaceDirectory::Root,
                Vec::new(),
                ProcessTimeout::from_millis(12_000)?,
                ProcessOutputLimit::new(512)?,
                ProcessOutputLimit::new(512)?,
                ProcessExecutionMode::Open,
                ProcessPlanBinding::Validated(TaskStepId::from_bytes([5; 32])),
                network,
            )?;
            let action = spec.policy_action();
            let at = AgentRunTimestamp::from_unix_millis(100)?;
            let request = ApprovalRequest::new(
                ApprovalRequestId::from_bytes([approval_seed; 32]),
                run,
                &action,
                at,
                AgentRunTimestamp::from_unix_millis(200)?,
            )?;
            let id = PolicyDecisionId::from_bytes([approval_seed; 32]);
            let mut grant =
                ApprovalGrant::grant(ApprovalId::from_bytes([approval_seed; 32]), &request, at)?;
            grant.consume(id, run, &action, at)?;
            let decision = PolicyDecision::approved(
                id,
                run,
                &action,
                &grant,
                PolicyEvaluationTiming::new(at, at)?,
            )?;
            Ok((AuthorizedProcessSpec::new(spec, &decision)?, id))
        }

        fn verify(&self, authorized: AuthorizedProcessSpec) -> StartOutcome {
            self.observe(authorized, StartCase::InvalidPort)
        }

        fn observe(&self, authorized: AuthorizedProcessSpec, case: StartCase) -> StartOutcome {
            observe_start(
                &self.project,
                &self.script,
                authorized,
                &self.environment,
                &Control,
                case,
            )
        }
    }

    fn bind_ephemeral() -> TestResult<(TcpSocket, NonZeroU16)> {
        let holder = TcpSocket::new_v4()?;
        holder.bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0).into())?;
        let port = NonZeroU16::new(holder.local_addr()?.port()).ok_or("no TCP port assigned")?;
        Ok((holder, port))
    }

    #[test]
    fn observes_real_invalid_port_exit_and_joins_group() -> TestResult {
        let fixture = Fixture::new(concat!(
            "from argparse import ArgumentParser\n",
            "from pathlib import Path\n",
            "Path('entered').touch()\n",
            "parser = ArgumentParser()\n",
            "parser.add_argument('--port', type=int)\n",
            "parser.parse_args()\n",
            "Path('parsed').touch()\n",
        ))?;
        let (authorized, decision_id) = fixture.authorize(ARGV)?;
        let specification_id = authorized.specification().specification_id();
        assert_eq!(
            fixture.verify(authorized),
            StartOutcome::Stopped(Ok(ObservedExit {
                specification_id,
                decision_id,
                code: 2
            })),
        );
        assert!(fixture.temp.path().join("entered").is_file());
        assert!(!fixture.temp.path().join("parsed").exists());
        Ok(())
    }

    #[test]
    fn preserves_zero_exit_and_does_not_report_a_signal_as_a_numeric_exit() -> TestResult {
        let fixture = Fixture::new("raise SystemExit(0)\n")?;
        let (authorized, decision_id) = fixture.authorize(ARGV)?;
        let specification_id = authorized.specification().specification_id();
        assert_eq!(
            fixture.verify(authorized),
            StartOutcome::Stopped(Ok(ObservedExit {
                specification_id,
                decision_id,
                code: 0
            })),
        );
        let fixture = Fixture::new("import os, signal\nos.kill(os.getpid(), signal.SIGTERM)\n")?;
        assert_eq!(
            fixture.verify(fixture.authorize(ARGV)?.0),
            StartOutcome::Stopped(Err(Failure::Exited))
        );
        Ok(())
    }

    #[test]
    fn rejects_approved_different_argv_and_stale_revision_before_spawn() -> TestResult {
        let marker = "from pathlib import Path\nPath('entered').touch()\nraise SystemExit(2)\n";
        let fixture = Fixture::new(marker)?;
        let (different, _) = fixture.authorize(["-B", SCRIPT, "--port", "18765"])?;
        assert_eq!(
            fixture.verify(different),
            StartOutcome::NotStarted(Failure::Denied)
        );
        fs::write(
            fixture.temp.path().join(SCRIPT),
            format!("# changed\n{marker}"),
        )?;
        assert_eq!(
            fixture.verify(fixture.authorize(ARGV)?.0),
            StartOutcome::NotStarted(Failure::StaleScript)
        );
        assert!(!fixture.temp.path().join("entered").exists());
        Ok(())
    }

    #[test]
    fn discards_exit_receipt_if_owned_process_changed_the_bound_script() -> TestResult {
        let fixture = Fixture::new(concat!(
            "from pathlib import Path\n",
            "Path(__file__).write_text('# changed\\n')\n",
            "raise SystemExit(2)\n",
        ))?;
        assert_eq!(
            fixture.verify(fixture.authorize(ARGV)?.0),
            StartOutcome::Stopped(Err(Failure::StaleScript))
        );
        assert_eq!(fs::read(fixture.temp.path().join(SCRIPT))?, b"# changed\n");
        Ok(())
    }

    #[test]
    fn exact_argv_rejects_the_held_endpoint_and_binds_when_released() -> TestResult {
        let fixture = Fixture::new(concat!(
            "from argparse import ArgumentParser\n",
            "import errno, socket\n",
            "parser = ArgumentParser()\n",
            "parser.add_argument('--host', required=True)\n",
            "parser.add_argument('--port', required=True, type=int)\n",
            "args = parser.parse_args()\n",
            "with socket.socket() as candidate:\n",
            "    candidate.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)\n",
            "    try:\n",
            "        candidate.bind((args.host, args.port))\n",
            "    except OSError as error:\n",
            "        if error.errno != errno.EADDRINUSE:\n",
            "            raise\n",
            "        raise SystemExit(7)\n",
        ))?;
        let (holder, port) = bind_ephemeral()?;
        drop(holder);
        let (blocked, blocked_decision) = fixture.authorize_explicit(port, 9)?;
        let (free, free_decision) = fixture.authorize_explicit(port, 10)?;
        let specification_id = blocked.specification().specification_id();
        assert_eq!(free.specification().specification_id(), specification_id);
        assert_ne!(blocked_decision, free_decision);
        assert_eq!(
            fixture.observe(blocked, StartCase::OccupiedExplicit(port)),
            StartOutcome::Stopped(Ok(ObservedExit {
                specification_id,
                decision_id: blocked_decision,
                code: 7,
            }))
        );
        assert_eq!(
            fixture.observe(free, StartCase::FreeExplicit(port)),
            StartOutcome::Stopped(Ok(ObservedExit {
                specification_id,
                decision_id: free_decision,
                code: 0,
            }))
        );
        Ok(())
    }

    #[test]
    fn foreign_binding_and_mismatched_port_do_not_start_python() -> TestResult {
        let fixture = Fixture::new("from pathlib import Path\nPath('entered').touch()\n")?;
        let (foreign, port) = bind_ephemeral()?;
        let (_other, other_port) = bind_ephemeral()?;
        assert_eq!(
            fixture.observe(
                fixture.authorize_explicit(port, 9)?.0,
                StartCase::OccupiedExplicit(other_port),
            ),
            StartOutcome::NotStarted(Failure::Denied)
        );
        assert_eq!(
            fixture.observe(
                fixture.authorize_explicit(port, 10)?.0,
                StartCase::OccupiedExplicit(port),
            ),
            StartOutcome::NotStarted(Failure::Occupied)
        );
        assert!(!fixture.temp.path().join("entered").exists());
        assert!(matches!(reserve_endpoint(port), Err(Failure::Occupied)));
        drop(foreign);
        assert!(reserve_endpoint(port).is_ok());
        Ok(())
    }

    #[test]
    fn zero_exit_during_occupied_start_does_not_prove_rejection() -> TestResult {
        let fixture = Fixture::new(concat!(
            "from pathlib import Path\n",
            "Path('entered').touch()\n",
            "raise SystemExit(0)\n",
        ))?;
        let (holder, port) = bind_ephemeral()?;
        drop(holder);
        assert_eq!(
            fixture.observe(
                fixture.authorize_explicit(port, 9)?.0,
                StartCase::OccupiedExplicit(port),
            ),
            StartOutcome::Stopped(Err(Failure::Exited))
        );
        assert!(fixture.temp.path().join("entered").is_file());
        assert!(reserve_endpoint(port).is_ok());
        Ok(())
    }

    #[test]
    fn cancellation_during_occupied_start_releases_the_endpoint_without_exit_proof() -> TestResult {
        let fixture = Fixture::new(concat!(
            "from pathlib import Path\n",
            "from time import sleep\n",
            "Path('entered').touch()\n",
            "sleep(60)\n",
        ))?;
        let (holder, port) = bind_ephemeral()?;
        drop(holder);
        let marker = fixture.temp.path().join("entered");
        assert_eq!(
            observe_start(
                &fixture.project,
                &fixture.script,
                fixture.authorize_explicit(port, 9)?.0,
                &fixture.environment,
                &CancelAfterEntry(marker.clone()),
                StartCase::OccupiedExplicit(port),
            ),
            StartOutcome::Stopped(Err(Failure::Cancelled))
        );
        assert!(marker.is_file());
        assert!(reserve_endpoint(port).is_ok());
        Ok(())
    }
}
