//! Owns the process group and the exact HTTP connections of a local verification recipe.

#[cfg(all(test, any(windows, target_os = "linux")))]
mod deadline_tests;
#[cfg(any(windows, target_os = "linux"))]
mod http;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(all(test, target_os = "linux"))]
mod negative_start;
#[cfg(any(windows, target_os = "linux"))]
mod startup;
#[cfg(windows)]
#[allow(unsafe_code)] // ADR-0122/0123 confine Win32 Job, TCP table and socket calls here.
mod windows_ffi;

use crate::ProcessHostEnvironment;
use a3_application::{
    AuthorizedLoopbackStartupCoverage, AuthorizedLoopbackVerification,
    LoopbackStartupVerificationFuture, LoopbackVerificationFailure as Failure,
    LoopbackVerificationFuture, LoopbackVerificationOutcome as Outcome, LoopbackVerificationRunner,
    ProcessRunControl,
};
use a3_domain::ProjectIdentity;

#[cfg(any(windows, target_os = "linux"))]
use {
    crate::process_launch::prepare_command,
    crate::secure_file::{SecureFileReadError, read_verified_text},
    a3_application::{LoopbackCleanupProof, LoopbackVerificationReport},
    a3_domain::{
        FileRevision, LOOPBACK_CLEANUP_RESERVE_MILLIS, MAX_LOOPBACK_PROCESS_MILLIS, ProcessSpec,
    },
    std::{
        net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    },
};

#[cfg(target_os = "linux")]
use crate::process_runner::spawn_process_group;
#[cfg(target_os = "linux")]
use command_group::GroupChild as LoopbackChild;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(windows)]
use windows_ffi as platform;
#[cfg(windows)]
use windows_ffi::{WindowsJobChild as LoopbackChild, WindowsJobSpawnFailure};

/// Closed loopback recipe adapter. Owns the child and observes only the approved requests.
#[derive(Debug)]
pub struct WorkspaceLoopbackVerificationRunner {
    environment: ProcessHostEnvironment,
}

impl WorkspaceLoopbackVerificationRunner {
    /// Uses the same explicit executable search environment as the normal process adapter.
    #[must_use]
    pub const fn new(environment: ProcessHostEnvironment) -> Self {
        Self { environment }
    }
}

impl LoopbackVerificationRunner for WorkspaceLoopbackVerificationRunner {
    fn supports_owned_cleanup(&self) -> bool {
        cfg!(any(windows, target_os = "linux"))
    }

    fn supports_startup_coverage(&self) -> bool {
        cfg!(any(windows, target_os = "linux"))
    }

    fn verify<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        authorized: AuthorizedLoopbackVerification,
        control: &'a dyn ProcessRunControl,
    ) -> LoopbackVerificationFuture<'a> {
        Box::pin(async move {
            #[cfg(any(windows, target_os = "linux"))]
            {
                thread::scope(|scope| {
                    let worker = thread::Builder::new()
                        .name("a3-loopback-verification".to_owned())
                        .spawn_scoped(scope, move || {
                            verify_sync(&self.environment, project, authorized, control)
                        });
                    match worker {
                        Ok(worker) => worker.join().unwrap_or(Outcome::UnknownCleanup),
                        Err(_) => Outcome::NotStarted(Failure::Unavailable),
                    }
                })
            }
            #[cfg(not(any(windows, target_os = "linux")))]
            {
                let _ = (&self.environment, project, authorized, control);
                Outcome::NotStarted(Failure::Unsupported)
            }
        })
    }

    fn verify_startup<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        authorized: AuthorizedLoopbackStartupCoverage,
        control: &'a dyn ProcessRunControl,
    ) -> LoopbackStartupVerificationFuture<'a> {
        Box::pin(async move {
            use a3_application::LoopbackStartupVerificationOutcome as StartupOutcome;
            #[cfg(any(windows, target_os = "linux"))]
            {
                thread::scope(|scope| {
                    let worker = thread::Builder::new()
                        .name("a3-loopback-startup-verification".to_owned())
                        .spawn_scoped(scope, move || {
                            startup::verify_sync(&self.environment, project, authorized, control)
                        });
                    match worker {
                        Ok(worker) => worker.join().unwrap_or(StartupOutcome::UnknownCleanup),
                        Err(_) => StartupOutcome::NotStarted(Failure::Unavailable),
                    }
                })
            }
            #[cfg(not(any(windows, target_os = "linux")))]
            {
                let _ = (&self.environment, project, authorized, control);
                StartupOutcome::NotStarted(Failure::Unsupported)
            }
        })
    }
}

#[cfg(any(windows, target_os = "linux"))]
const MAX_RECIPE_TIME: Duration = Duration::from_millis(MAX_LOOPBACK_PROCESS_MILLIS);
#[cfg(any(windows, target_os = "linux"))]
const CLEANUP_RESERVE: Duration = Duration::from_millis(LOOPBACK_CLEANUP_RESERVE_MILLIS);
#[cfg(any(windows, target_os = "linux"))]
const POLL: Duration = Duration::from_millis(10);

#[cfg(any(windows, target_os = "linux"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ownership {
    Absent,
    Owned,
    Foreign,
}

#[cfg(any(windows, target_os = "linux"))]
fn verify_sync(
    environment: &ProcessHostEnvironment,
    project: &ProjectIdentity,
    authorized: AuthorizedLoopbackVerification,
    control: &dyn ProcessRunControl,
) -> Outcome {
    let (recipe, start, decisions) = authorized.into_parts();
    let total = Duration::from_millis(start.specification().timeout().as_millis());
    if total <= CLEANUP_RESERVE || total > MAX_RECIPE_TIME {
        return Outcome::NotStarted(Failure::Denied);
    }
    let endpoint = SocketAddrV4::new(Ipv4Addr::LOCALHOST, recipe.port().get());
    let end = Instant::now() + total;
    let probe_end = end - CLEANUP_RESERVE;
    let prepared = (|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| Failure::Unavailable)?;
        let command = prepare_loopback_command(project, start.specification(), environment)?;
        if !observe_until(probe_end, control, |time| {
            platform::listener_absent(endpoint, time)
        })? {
            return Err(Failure::Occupied);
        }
        check_revision(project, recipe.script(), control)?;
        remaining(probe_end, control)?;
        Ok((runtime, command))
    })();
    let (runtime, command) = match prepared {
        Ok(prepared) => prepared,
        Err(failure) => return Outcome::NotStarted(failure),
    };

    let (_, start_decision) = start.into_parts();
    let mut child = match start_owned(command) {
        Ok(child) => child,
        Err(outcome) => return outcome,
    };
    let result = (|| {
        let mut observations = Vec::with_capacity(decisions.len());
        for (check, decision) in recipe.recipe().checks().iter().zip(decisions) {
            let stream = owned_stream(&child, endpoint, probe_end, control)?;
            observations.push(runtime.block_on(http::probe(
                stream,
                endpoint,
                check.probe(),
                decision,
                remaining(probe_end, control)?,
                control,
            ))?);
        }
        wait_owned(&child, endpoint, None, probe_end, control)?;
        Ok(LoopbackVerificationReport {
            start_decision,
            probes: observations,
        })
    })();

    // Cancellation stops probes, not the bounded cleanup of effects already admitted.
    let Some(cleanup) = stop(&mut child, endpoint, end) else {
        return Outcome::UnknownCleanup;
    };
    let result = result.and_then(|report| {
        check_revision(project, recipe.script(), control)?;
        remaining(end, control)?;
        Ok(report)
    });
    Outcome::Stopped { cleanup, result }
}

#[cfg(any(windows, target_os = "linux"))]
fn prepare_loopback_command(
    project: &ProjectIdentity,
    specification: &ProcessSpec,
    environment: &ProcessHostEnvironment,
) -> Result<Command, Failure> {
    let mut command =
        prepare_command(project, specification, environment).map_err(|_| Failure::Denied)?;
    if std::path::Path::new(command.get_program()).starts_with(project.worktree().root().as_path())
    {
        return Err(Failure::Denied);
    }
    #[cfg(windows)]
    {
        if specification.environment_allowlist().is_empty() {
            return Err(Failure::Denied);
        }
        command.env("SYSTEMROOT", windows_system_root()?);
    }
    #[cfg(target_os = "linux")]
    if !specification.environment_allowlist().is_empty() {
        return Err(Failure::Denied);
    }
    // No piped readers or untrusted diagnostic text can outlive the owned group.
    command.stdout(Stdio::null()).stderr(Stdio::null());
    Ok(command)
}

#[cfg(target_os = "linux")]
fn start_owned(mut command: Command) -> Result<LoopbackChild, Outcome> {
    start_borrowed_owned(&mut command)
}

#[cfg(windows)]
fn start_owned(command: Command) -> Result<LoopbackChild, Outcome> {
    start_borrowed_owned(&command)
}

#[cfg(target_os = "linux")]
fn start_borrowed_owned(command: &mut Command) -> Result<LoopbackChild, Outcome> {
    spawn_process_group(command).map_err(|_| Outcome::UnknownCleanup)
}

#[cfg(windows)]
fn start_borrowed_owned(command: &Command) -> Result<LoopbackChild, Outcome> {
    LoopbackChild::spawn(command).map_err(|error| match error {
        WindowsJobSpawnFailure::NotStarted => Outcome::NotStarted(Failure::Unavailable),
        WindowsJobSpawnFailure::Unknown => Outcome::UnknownCleanup,
    })
}

#[cfg(windows)]
fn windows_system_root() -> Result<std::path::PathBuf, Failure> {
    // GLOBALROOT is independent of the worktree, search path and inherited environment.
    std::fs::canonicalize(r"\\?\GLOBALROOT\SystemRoot\System32")
        .ok()
        .and_then(|directory| directory.parent().map(std::path::Path::to_path_buf))
        .ok_or(Failure::Denied)
}

#[cfg(any(windows, target_os = "linux"))]
fn check_revision(
    project: &ProjectIdentity,
    script: &FileRevision,
    control: &dyn ProcessRunControl,
) -> Result<(), Failure> {
    read_verified_text(project.worktree().root().as_path(), script, || {
        control.is_cancelled()
    })
    .map(|_| ())
    .map_err(|error| match error {
        SecureFileReadError::Cancelled => Failure::Cancelled,
        SecureFileReadError::Stale => Failure::StaleScript,
        _ => Failure::Denied,
    })
}

#[cfg(any(windows, target_os = "linux"))]
fn owned_stream(
    child: &LoopbackChild,
    endpoint: SocketAddrV4,
    end: Instant,
    control: &dyn ProcessRunControl,
) -> Result<TcpStream, Failure> {
    wait_owned(child, endpoint, None, end, control)?;
    let stream = observe_until(end, control, |time| {
        TcpStream::connect_timeout(
            &SocketAddr::V4(endpoint),
            time.min(Duration::from_millis(250)),
        )
    })?;
    let SocketAddr::V4(peer) = stream.local_addr().map_err(|_| Failure::Unavailable)? else {
        return Err(Failure::Unavailable);
    };
    wait_owned(child, endpoint, Some(peer), end, control)?;
    stream
        .set_nonblocking(true)
        .map_err(|_| Failure::Unavailable)?;
    Ok(stream)
}

#[cfg(any(windows, target_os = "linux"))]
fn wait_owned(
    child: &LoopbackChild,
    endpoint: SocketAddrV4,
    peer: Option<SocketAddrV4>,
    end: Instant,
    control: &dyn ProcessRunControl,
) -> Result<(), Failure> {
    loop {
        // On Linux the unreaped child prevents its process-group ID from being reused
        // before cleanup. The proc observer checks that the direct child is still live.
        #[cfg(windows)]
        require_running(child, end, control)?;
        let ownership = observe_until(end, control, |time| {
            platform::observe(child.id(), endpoint, peer, time)
        })?;
        #[cfg(windows)]
        require_running(child, end, control)?;
        match ownership {
            Ownership::Owned => return remaining(end, control).map(|_| ()),
            Ownership::Foreign => return Err(Failure::Occupied),
            Ownership::Absent => {
                if control.wait_cancelled_timeout(remaining(end, control)?.min(POLL)) {
                    return Err(Failure::Cancelled);
                }
            }
        }
    }
}

#[cfg(windows)]
fn require_running(
    child: &LoopbackChild,
    end: Instant,
    control: &dyn ProcessRunControl,
) -> Result<(), Failure> {
    if observe_until(end, control, |_| child.is_running())? {
        Ok(())
    } else {
        Err(Failure::Exited)
    }
}

#[cfg(target_os = "linux")]
fn stop(
    child: &mut LoopbackChild,
    endpoint: SocketAddrV4,
    end: Instant,
) -> Option<LoopbackCleanupProof> {
    stop_owned_group(child, end)?;
    loop {
        if platform::listener_absent(endpoint, time_until(end)?).ok()? {
            time_until(end)?;
            return Some(LoopbackCleanupProof::ProcessGroupAndListener);
        }
        thread::sleep(time_until(end)?.min(POLL));
    }
}

#[cfg(target_os = "linux")]
fn stop_owned_group(child: &mut LoopbackChild, end: Instant) -> Option<()> {
    let killed = child.kill();
    if let Err(error) = killed
        && error.raw_os_error() != Some(rustix::io::Errno::SRCH.raw_os_error())
    {
        return None;
    }
    loop {
        time_until(end)?;
        let stopped = child.try_wait().ok()?.is_some() && group_stopped(child.id())?;
        if stopped {
            time_until(end)?;
            return Some(());
        }
        thread::sleep(time_until(end)?.min(POLL));
    }
}

#[cfg(windows)]
fn stop(
    child: &mut LoopbackChild,
    endpoint: SocketAddrV4,
    end: Instant,
) -> Option<LoopbackCleanupProof> {
    stop_owned_group(child, end)?;
    loop {
        if platform::listener_absent(endpoint, time_until(end)?).ok()? {
            time_until(end)?;
            return Some(LoopbackCleanupProof::WindowsJobAndListener);
        }
        thread::sleep(time_until(end)?.min(POLL));
    }
}

#[cfg(windows)]
fn stop_owned_group(child: &mut LoopbackChild, end: Instant) -> Option<()> {
    child.stop(end).ok()
}

#[cfg(target_os = "linux")]
fn group_stopped(pid: u32) -> Option<bool> {
    let pid = rustix::process::Pid::from_raw(i32::try_from(pid).ok()?)?;
    match rustix::process::test_kill_process_group(pid) {
        Err(rustix::io::Errno::SRCH) => Some(true),
        Ok(()) => Some(false),
        Err(_) => None,
    }
}

#[cfg(any(windows, target_os = "linux"))]
fn remaining(end: Instant, control: &dyn ProcessRunControl) -> Result<Duration, Failure> {
    if control.is_cancelled() {
        return Err(Failure::Cancelled);
    }
    time_until(end).ok_or(Failure::TimedOut)
}

#[cfg(any(windows, target_os = "linux"))]
fn observe_until<T, E>(
    end: Instant,
    control: &dyn ProcessRunControl,
    observe: impl FnOnce(Duration) -> Result<T, E>,
) -> Result<T, Failure> {
    let observed = observe(remaining(end, control)?).map_err(|_| Failure::Unavailable);
    finish_observation(observed, end, control)
}

#[cfg(any(windows, target_os = "linux"))]
fn finish_observation<T>(
    observed: Result<T, Failure>,
    end: Instant,
    control: &dyn ProcessRunControl,
) -> Result<T, Failure> {
    // OS observers may report an internal timeout as an unavailable observation.
    // Recheck the original effect deadline before accepting either their result or error.
    remaining(end, control)?;
    observed
}

#[cfg(any(windows, target_os = "linux"))]
fn time_until(end: Instant) -> Option<Duration> {
    let time = end.saturating_duration_since(Instant::now());
    (!time.is_zero()).then_some(time)
}
