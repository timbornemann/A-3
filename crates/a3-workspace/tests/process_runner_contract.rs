//! Public cross-platform ProcessRunner contract for the concrete workspace adapter.

mod support;

use a3_application::{
    AuthorizedProcessSpec, ProcessEventSink, ProcessEventSinkError, ProcessRunControl,
    ProcessRunFailure, ProcessRunner,
};
use a3_domain::{
    AgentRunId, AgentRunTimestamp, CanonicalDirectory, GitHead, GitReferenceName, PolicyDecision,
    PolicyDecisionId, PolicyEvaluationTiming, ProcessArgument, ProcessEnvironmentVariable,
    ProcessEvent, ProcessEventKind, ProcessExecutable, ProcessExecutionMode, ProcessNetworkScope,
    ProcessOutputLimit, ProcessPlanBinding, ProcessSpec, ProcessSpecSchemaVersion, ProcessStream,
    ProcessTermination, ProcessTimeout, ProjectIdentity, RepositoryId, RepositoryIdentity,
    RepositoryPath, TaskStepId, WorkspaceDirectory, WorktreeAnchorId, WorktreeId, WorktreeIdentity,
};
use a3_workspace::{ProcessHostEnvironment, WorkspaceProcessRunner};
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use support::TempDirectory;

const SHELL_CHARACTERS: &str = "literal;$(not-executed) && | > < `still-literal`";

#[test]
fn direct_argv_cwd_executable_and_environment_policy_are_enforced() -> Result<(), Box<dyn Error>> {
    let fixture = ProcessFixture::new()?;
    let runner = WorkspaceProcessRunner::new(fixture.environment()?);

    let (echo, events) = fixture.run(
        &runner,
        vec!["echo", SHELL_CHARACTERS],
        WorkspaceDirectory::Root,
        Vec::new(),
        5_000,
        16 * 1_024,
        &ActiveControl,
    )?;
    let expected_echo = format!("{SHELL_CHARACTERS}\n");
    assert_eq!(
        echo.stdout().content().as_text(),
        Some(expected_echo.as_str())
    );
    assert_success(&echo.termination())?;
    assert_event_contract(
        &events,
        echo.specification_id(),
        ProcessTerminationKind::Exited,
    )?;

    let package = fixture.root().join("packages").join("one");
    fs::create_dir_all(&package)?;
    fs::write(package.join("marker.txt"), "package-one")?;
    let (cwd, _) = fixture.run(
        &runner,
        vec!["cwd"],
        WorkspaceDirectory::Subtree(RepositoryPath::try_from_bytes(b"packages/one".to_vec())?),
        Vec::new(),
        5_000,
        16 * 1_024,
        &ActiveControl,
    )?;
    assert_eq!(cwd.stdout().content().as_text(), Some("package-one"));

    let allowed = ProcessEnvironmentVariable::try_from_string("A3_ALLOWED".to_owned())?;
    let (environment, _) = fixture.run(
        &runner,
        vec!["environment"],
        WorkspaceDirectory::Root,
        vec![allowed],
        5_000,
        16 * 1_024,
        &ActiveControl,
    )?;
    assert_eq!(
        environment.stdout().content().as_text(),
        Some("allowed-value;path=false\n")
    );

    let denied = fixture.specification(
        ProcessExecutable::try_from_string("../process-fixture".to_owned())?,
        vec!["echo", "denied"],
        WorkspaceDirectory::Root,
        Vec::new(),
        5_000,
        1_024,
    )?;
    let decision = automatic_decision(&denied)?;
    let result = futures::executor::block_on(runner.run(
        &fixture.project,
        AuthorizedProcessSpec::new(denied, &decision)?,
        &ActiveControl,
        &RecordingEvents::default(),
    ));
    assert_eq!(result, Err(ProcessRunFailure::Denied));
    Ok(())
}

#[test]
fn endless_process_group_is_killed_at_timeout() -> Result<(), Box<dyn Error>> {
    let fixture = ProcessFixture::new()?;
    let runner = WorkspaceProcessRunner::new(fixture.environment()?);
    let started = Instant::now();
    let (result, events) = fixture.run(
        &runner,
        vec!["hang"],
        WorkspaceDirectory::Root,
        Vec::new(),
        200,
        1_024,
        &ActiveControl,
    )?;
    assert_eq!(result.termination(), ProcessTermination::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_event_contract(
        &events,
        result.specification_id(),
        ProcessTerminationKind::TimedOut,
    )?;
    Ok(())
}

#[test]
fn cancellation_kills_the_spawned_child_process() -> Result<(), Box<dyn Error>> {
    let fixture = ProcessFixture::new()?;
    let runner = WorkspaceProcessRunner::new(fixture.environment()?);
    let pid_file = fixture.root().join("child.pid");
    let control = CancellableControl::default();
    let cancellation = control.clone();
    let observed_pid_file = pid_file.clone();
    let canceller = thread::Builder::new()
        .name("a3-process-contract-canceller".to_owned())
        .spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !observed_pid_file.exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            cancellation.cancel();
        })?;
    let pid_argument = pid_file.to_str().ok_or("pid path is not UTF-8")?;
    let (result, events) = fixture.run(
        &runner,
        vec!["spawn-child", pid_argument],
        WorkspaceDirectory::Root,
        Vec::new(),
        10_000,
        1_024,
        &control,
    )?;
    if canceller.join().is_err() {
        return Err("cancellation helper panicked".into());
    }
    assert_eq!(result.termination(), ProcessTermination::Cancelled);
    let pid: u32 = fs::read_to_string(&pid_file)?.trim().parse()?;
    assert_process_stopped(pid)?;
    assert_event_contract(
        &events,
        result.specification_id(),
        ProcessTerminationKind::Cancelled,
    )?;
    Ok(())
}

#[test]
fn output_overflow_is_drained_without_blocking_the_process() -> Result<(), Box<dyn Error>> {
    let fixture = ProcessFixture::new()?;
    let runner = WorkspaceProcessRunner::new(fixture.environment()?);
    let bytes = 2 * 1_024 * 1_024;
    let (result, events) = fixture.run(
        &runner,
        vec!["overflow", &bytes.to_string()],
        WorkspaceDirectory::Root,
        Vec::new(),
        10_000,
        1_024,
        &ActiveControl,
    )?;
    assert_success(&result.termination())?;
    assert_eq!(result.stdout().observed_bytes(), bytes);
    assert_eq!(
        result.stdout().content().as_text().map(str::len),
        Some(1_024)
    );
    assert!(result.stdout().truncated());
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event.kind(),
                ProcessEventKind::OutputTruncated {
                    stream: ProcessStream::Stdout,
                    ..
                }
            ))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn secret_output_is_redacted_before_result_and_stream_events() -> Result<(), Box<dyn Error>> {
    let fixture = ProcessFixture::new()?;
    let runner = WorkspaceProcessRunner::new(fixture.environment()?);
    let (result, events) = fixture.run(
        &runner,
        vec!["secret-output"],
        WorkspaceDirectory::Root,
        Vec::new(),
        5_000,
        1_024,
        &ActiveControl,
    )?;
    assert_success(&result.termination())?;
    assert_eq!(
        result.stdout().content().redaction(),
        Some(a3_domain::ProcessOutputRedaction::SecretCandidate)
    );
    assert!(result.stdout().content().as_text().is_none());
    assert!(events.iter().any(|event| matches!(
        event.kind(),
        ProcessEventKind::OutputRedacted {
            stream: ProcessStream::Stdout,
            reason: a3_domain::ProcessOutputRedaction::SecretCandidate,
        }
    )));
    assert!(!events.iter().any(|event| matches!(
        event.kind(),
        ProcessEventKind::Output {
            stream: ProcessStream::Stdout,
            ..
        }
    )));
    Ok(())
}

#[test]
fn event_backpressure_failure_stops_the_owned_process_group() -> Result<(), Box<dyn Error>> {
    let fixture = ProcessFixture::new()?;
    let runner = WorkspaceProcessRunner::new(fixture.environment()?);
    let specification = fixture.specification(
        ProcessExecutable::try_from_string(
            fixture
                .executable
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .ok_or("fixture executable name is not UTF-8")?
                .to_owned(),
        )?,
        vec!["hang"],
        WorkspaceDirectory::Root,
        Vec::new(),
        10_000,
        1_024,
    )?;
    let decision = automatic_decision(&specification)?;
    let started = Instant::now();
    let result = futures::executor::block_on(runner.run(
        &fixture.project,
        AuthorizedProcessSpec::new(specification, &decision)?,
        &ActiveControl,
        &RejectingEvents,
    ));
    assert_eq!(result, Err(ProcessRunFailure::EventUnavailable));
    assert!(started.elapsed() < Duration::from_secs(5));
    Ok(())
}

struct ProcessFixture {
    _temporary: TempDirectory,
    project: ProjectIdentity,
    executable: PathBuf,
}

impl ProcessFixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let temporary = TempDirectory::new()?;
        let root = temporary.path().join("selected");
        fs::create_dir(&root)?;
        let executable = compile_process_fixture(temporary.path())?;
        let canonical_root = CanonicalDirectory::from_canonicalized(fs::canonicalize(&root)?)?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        let project = ProjectIdentity::new(
            RepositoryIdentity::new(repository_id, canonical_root.clone(), None),
            WorktreeIdentity::new(
                WorktreeId::from_bytes([2; 32]),
                WorktreeAnchorId::from_bytes([3; 32]),
                repository_id,
                canonical_root,
            ),
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?;
        Ok(Self {
            _temporary: temporary,
            project,
            executable,
        })
    }

    fn root(&self) -> &Path {
        self.project.worktree().root().as_path()
    }

    fn environment(&self) -> Result<ProcessHostEnvironment, Box<dyn Error>> {
        let path = ProcessEnvironmentVariable::try_from_string("PATH".to_owned())?;
        let allowed = ProcessEnvironmentVariable::try_from_string("A3_ALLOWED".to_owned())?;
        let executable_parent = self.executable.parent().ok_or("fixture has no parent")?;
        Ok(ProcessHostEnvironment::new(vec![
            (path, std::env::join_paths([executable_parent])?),
            (allowed, OsString::from("allowed-value")),
        ])?)
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        runner: &WorkspaceProcessRunner,
        arguments: Vec<&str>,
        working_directory: WorkspaceDirectory,
        environment: Vec<ProcessEnvironmentVariable>,
        timeout_millis: u64,
        output_limit: u32,
        control: &dyn ProcessRunControl,
    ) -> Result<(a3_domain::ProcessRunResult, Vec<ProcessEvent>), Box<dyn Error>> {
        let executable_name = self
            .executable
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or("fixture executable name is not UTF-8")?;
        let specification = self.specification(
            ProcessExecutable::try_from_string(executable_name.to_owned())?,
            arguments,
            working_directory,
            environment,
            timeout_millis,
            output_limit,
        )?;
        let decision = automatic_decision(&specification)?;
        let events = RecordingEvents::default();
        let result = futures::executor::block_on(runner.run(
            &self.project,
            AuthorizedProcessSpec::new(specification, &decision)?,
            control,
            &events,
        ))?;
        Ok((result, events.take()))
    }

    #[allow(clippy::too_many_arguments)]
    fn specification(
        &self,
        executable: ProcessExecutable,
        arguments: Vec<&str>,
        working_directory: WorkspaceDirectory,
        environment: Vec<ProcessEnvironmentVariable>,
        timeout_millis: u64,
        output_limit: u32,
    ) -> Result<ProcessSpec, Box<dyn Error>> {
        let arguments = arguments
            .into_iter()
            .map(|argument| ProcessArgument::try_from_string(argument.to_owned()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ProcessSpec::new(
            ProcessSpecSchemaVersion::V1,
            AgentRunId::from_bytes([4; 32]),
            self.project.worktree().id(),
            executable,
            arguments,
            working_directory,
            environment,
            ProcessTimeout::from_millis(timeout_millis)?,
            ProcessOutputLimit::new(output_limit)?,
            ProcessOutputLimit::new(output_limit)?,
            ProcessExecutionMode::KnownSafe,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([5; 32])),
            ProcessNetworkScope::Denied,
        )?)
    }
}

fn compile_process_fixture(output_directory: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("process_fixture.rs");
    let executable = output_directory.join(if cfg!(windows) {
        "a3-process-fixture.exe"
    } else {
        "a3-process-fixture"
    });
    let status = Command::new("rustc")
        .args(["--edition=2024", "--crate-name", "a3_process_fixture"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()?;
    if !status.success() {
        return Err("could not compile process contract fixture".into());
    }
    Ok(fs::canonicalize(executable)?)
}

fn automatic_decision(specification: &ProcessSpec) -> Result<PolicyDecision, Box<dyn Error>> {
    let timestamp = AgentRunTimestamp::from_unix_millis(1)?;
    Ok(PolicyDecision::automatic(
        PolicyDecisionId::from_bytes([6; 32]),
        specification.run_id(),
        &specification.policy_action(),
        PolicyEvaluationTiming::new(timestamp, timestamp)?,
    ))
}

fn assert_success(termination: &ProcessTermination) -> Result<(), Box<dyn Error>> {
    match termination {
        ProcessTermination::Exited(exit) if exit.success() && exit.code() == Some(0) => Ok(()),
        _ => Err("process did not exit successfully".into()),
    }
}

#[derive(Clone, Copy)]
enum ProcessTerminationKind {
    Exited,
    TimedOut,
    Cancelled,
}

fn assert_event_contract(
    events: &[ProcessEvent],
    specification_id: a3_domain::PolicyResourceId,
    terminal: ProcessTerminationKind,
) -> Result<(), Box<dyn Error>> {
    if events.is_empty() || !matches!(events[0].kind(), ProcessEventKind::Started) {
        return Err("process event stream did not start with Started".into());
    }
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event.specification_id(), specification_id);
        assert_eq!(
            event.sequence().get(),
            u64::try_from(index)?.saturating_add(1)
        );
    }
    let final_event = events.last().ok_or("process event stream is empty")?;
    let matches_terminal = matches!(
        (final_event.kind(), terminal),
        (
            ProcessEventKind::Terminated(ProcessTermination::Exited(_)),
            ProcessTerminationKind::Exited
        ) | (
            ProcessEventKind::Terminated(ProcessTermination::TimedOut),
            ProcessTerminationKind::TimedOut
        ) | (
            ProcessEventKind::Terminated(ProcessTermination::Cancelled),
            ProcessTerminationKind::Cancelled
        )
    );
    if !matches_terminal {
        return Err("process event stream has the wrong terminal event".into());
    }
    Ok(())
}

#[derive(Debug)]
struct ActiveControl;

impl ProcessRunControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn wait_cancelled_timeout(&self, timeout: Duration) -> bool {
        if !timeout.is_zero() {
            thread::sleep(timeout);
        }
        false
    }
}

#[derive(Debug, Clone, Default)]
struct CancellableControl {
    state: Arc<(Mutex<bool>, Condvar)>,
}

impl CancellableControl {
    fn cancel(&self) {
        let (cancelled, changed) = &*self.state;
        *lock_recovering_poison(cancelled) = true;
        changed.notify_all();
    }
}

impl ProcessRunControl for CancellableControl {
    fn is_cancelled(&self) -> bool {
        *lock_recovering_poison(&self.state.0)
    }

    fn wait_cancelled_timeout(&self, timeout: Duration) -> bool {
        let (cancelled, changed) = &*self.state;
        let guard = lock_recovering_poison(cancelled);
        if *guard {
            return true;
        }
        let result = changed.wait_timeout_while(guard, timeout, |value| !*value);
        match result {
            Ok((state, _)) => *state,
            Err(poisoned) => *poisoned.into_inner().0,
        }
    }
}

#[derive(Debug, Default)]
struct RecordingEvents {
    events: Mutex<Vec<ProcessEvent>>,
}

impl RecordingEvents {
    fn take(&self) -> Vec<ProcessEvent> {
        std::mem::take(&mut *lock_recovering_poison(&self.events))
    }
}

impl ProcessEventSink for RecordingEvents {
    fn emit(&self, event: ProcessEvent) -> Result<(), ProcessEventSinkError> {
        lock_recovering_poison(&self.events).push(event);
        Ok(())
    }
}

#[derive(Debug)]
struct RejectingEvents;

impl ProcessEventSink for RejectingEvents {
    fn emit(&self, _event: ProcessEvent) -> Result<(), ProcessEventSinkError> {
        Err(ProcessEventSinkError)
    }
}

fn lock_recovering_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn assert_process_stopped(pid: u32) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_exists(pid)? && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if process_exists(pid)? {
        return Err("child process remained alive after cancellation".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn process_exists(pid: u32) -> Result<bool, Box<dyn Error>> {
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let command_end = stat.rfind(')').ok_or("Linux process stat is malformed")?;
    let state = stat
        .get(command_end.saturating_add(2)..)
        .and_then(|suffix| suffix.chars().next())
        .ok_or("Linux process state is missing")?;
    Ok(!matches!(state, 'Z' | 'X' | 'x'))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_exists(pid: u32) -> Result<bool, Box<dyn Error>> {
    let status = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()?;
    Ok(status.success())
}

#[cfg(windows)]
fn process_exists(pid: u32) -> Result<bool, Box<dyn Error>> {
    let system_root = std::env::var_os("SystemRoot").ok_or("SystemRoot is unavailable")?;
    let tasklist = PathBuf::from(system_root)
        .join("System32")
        .join("tasklist.exe");
    let output = Command::new(tasklist)
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()?;
    if !output.status.success() {
        return Err("tasklist failed".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")))
}
