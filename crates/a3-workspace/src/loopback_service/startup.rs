//! Executes all original-bound startup cases as one owned, bounded tool invocation.

use super::*;
use a3_application::{
    LoopbackStartupCleanupProof, LoopbackStartupVerificationOutcome as StartupOutcome,
    LoopbackStartupVerificationReport, PreparedLoopbackStartupCoverage,
};
use a3_domain::{
    LOOPBACK_DEFAULT_PORT, LoopbackFailureExitCode, LoopbackHttpCheck, LoopbackHttpObservation,
    LoopbackStartupPhase, PolicyDecisionId,
};
use std::io::ErrorKind;

enum Stage<T> {
    Clean(Result<T, Failure>),
    Mismatched(a3_domain::LoopbackHttpMismatch),
    Unknown,
}

pub(super) fn verify_sync(
    environment: &ProcessHostEnvironment,
    project: &ProjectIdentity,
    authorized: AuthorizedLoopbackStartupCoverage,
    control: &dyn ProcessRunControl,
) -> StartupOutcome {
    let (prepared, decisions) = authorized.into_parts();
    let total = Duration::from_millis(prepared.settings().timeout.as_millis());
    if total <= CLEANUP_RESERVE || total > MAX_RECIPE_TIME {
        return StartupOutcome::NotStarted(Failure::Denied);
    }
    let Some(end) = Instant::now().checked_add(total) else {
        return StartupOutcome::NotStarted(Failure::Denied);
    };
    let effect_end = end - CLEANUP_RESERVE;
    let explicit_endpoint = SocketAddrV4::new(Ipv4Addr::LOCALHOST, prepared.recipe().port().get());
    let default_endpoint = SocketAddrV4::new(Ipv4Addr::LOCALHOST, LOOPBACK_DEFAULT_PORT);
    let preflight = (|| {
        remaining(effect_end, control)?;
        let explicit = prepared
            .process(LoopbackStartupPhase::FreeExplicit)
            .ok_or(Failure::Denied)?;
        if Some(explicit) != prepared.process(LoopbackStartupPhase::HeldExplicit) {
            return Err(Failure::Denied);
        }
        let default = prepared
            .process(LoopbackStartupPhase::Default)
            .ok_or(Failure::Denied)?;
        let invalid = prepared
            .process(LoopbackStartupPhase::InvalidPort)
            .ok_or(Failure::Denied)?;
        let explicit_command = prepare_loopback_command(project, explicit, environment)?;
        let default_command = prepare_loopback_command(project, default, environment)?;
        let invalid_command = prepare_loopback_command(project, invalid, environment)?;
        if explicit_command.get_program() != default_command.get_program()
            || explicit_command.get_program() != invalid_command.get_program()
        {
            return Err(Failure::Denied);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| Failure::Unavailable)?;
        for endpoint in [explicit_endpoint, default_endpoint] {
            if !observe_until(effect_end, control, |time| {
                platform::listener_absent(endpoint, time)
            })? {
                return Err(Failure::Occupied);
            }
        }
        let default_index = prepared
            .recipe()
            .default_start_check_index()
            .ok_or(Failure::Denied)?;
        let default_check = prepared
            .recipe()
            .checks()
            .get(usize::from(default_index))
            .ok_or(Failure::Denied)?;
        if decisions.explicit_probes.len() != prepared.recipe().checks().len() {
            return Err(Failure::Denied);
        }
        check_revision(project, prepared.script(), control)?;
        remaining(effect_end, control)?;
        Ok((
            runtime,
            explicit_command,
            default_command,
            invalid_command,
            default_check,
        ))
    })();
    let (runtime, mut explicit, mut default, mut invalid, default_check) = match preflight {
        Ok(prepared) => prepared,
        Err(failure) => return StartupOutcome::NotStarted(failure),
    };
    let reservation = match platform::ExclusiveLoopbackBind::bind(prepared.recipe().port()) {
        Ok(reservation) => reservation,
        Err(error) => {
            return StartupOutcome::NotStarted(match error.kind() {
                ErrorKind::AddrInUse => Failure::Occupied,
                _ => Failure::Unavailable,
            });
        }
    };

    // The successfully bound endpoint is the first externally observable effect.
    let held_exit =
        match rejected_start(project, &prepared, &mut explicit, effect_end, end, control) {
            Stage::Clean(Ok(exit)) => exit,
            Stage::Clean(Err(failure)) => {
                drop(reservation);
                return stopped(Err(failure));
            }
            Stage::Unknown => {
                hold_until(end);
                drop(reservation);
                return StartupOutcome::UnknownCleanup;
            }
            Stage::Mismatched(_) => return stopped(Err(Failure::Denied)),
        };
    drop(reservation);
    let explicit_probes = match positive_start(
        project,
        &prepared,
        &mut explicit,
        explicit_endpoint,
        prepared.recipe().checks(),
        &decisions.explicit_probes,
        LoopbackStartupPhase::FreeExplicit,
        &runtime,
        effect_end,
        end,
        control,
    ) {
        Stage::Clean(Ok(probes)) => probes,
        Stage::Clean(Err(failure)) => return stopped(Err(failure)),
        Stage::Unknown => return StartupOutcome::UnknownCleanup,
        Stage::Mismatched(mismatch) => return mismatched(mismatch),
    };
    let default_probes = match positive_start(
        project,
        &prepared,
        &mut default,
        default_endpoint,
        std::slice::from_ref(default_check),
        &[decisions.default_probe],
        LoopbackStartupPhase::Default,
        &runtime,
        effect_end,
        end,
        control,
    ) {
        Stage::Clean(Ok(probes)) => probes,
        Stage::Clean(Err(failure)) => return stopped(Err(failure)),
        Stage::Unknown => return StartupOutcome::UnknownCleanup,
        Stage::Mismatched(mismatch) => return mismatched(mismatch),
    };
    let Some(default_probe) = default_probes.into_iter().next() else {
        return stopped(Err(Failure::InvalidResponse));
    };
    let invalid_exit =
        match rejected_start(project, &prepared, &mut invalid, effect_end, end, control) {
            Stage::Clean(Ok(exit)) => exit,
            Stage::Clean(Err(failure)) => return stopped(Err(failure)),
            Stage::Unknown => return StartupOutcome::UnknownCleanup,
            Stage::Mismatched(_) => return stopped(Err(Failure::Denied)),
        };
    let result = (|| {
        check_revision(project, prepared.script(), control)?;
        remaining(end, control)?;
        Ok(LoopbackStartupVerificationReport {
            decisions,
            held_exit,
            invalid_exit,
            explicit_probes,
            default_probe,
        })
    })();
    stopped(result)
}

fn rejected_start(
    project: &ProjectIdentity,
    prepared: &PreparedLoopbackStartupCoverage,
    command: &mut Command,
    effect_end: Instant,
    end: Instant,
    control: &dyn ProcessRunControl,
) -> Stage<LoopbackFailureExitCode> {
    let ready = check_revision(project, prepared.script(), control)
        .and_then(|()| remaining(effect_end, control).map(|_| ()));
    if let Err(failure) = ready {
        return Stage::Clean(Err(failure));
    }
    let mut child = match start_borrowed_owned(command) {
        Ok(child) => child,
        Err(Outcome::NotStarted(failure)) => return Stage::Clean(Err(failure)),
        Err(_) => return Stage::Unknown,
    };
    let observed = match observe_failure_exit(&child, effect_end, control) {
        Stage::Clean(result) => result,
        Stage::Unknown => return Stage::Unknown,
        Stage::Mismatched(_) => return Stage::Clean(Err(Failure::Denied)),
    };
    if stop_owned_group(&mut child, end).is_none() {
        return Stage::Unknown;
    }
    let current = check_revision(project, prepared.script(), control)
        .and_then(|()| remaining(end, control).map(|_| ()));
    Stage::Clean(current.and(observed))
}

#[allow(clippy::too_many_arguments)]
fn positive_start(
    project: &ProjectIdentity,
    prepared: &PreparedLoopbackStartupCoverage,
    command: &mut Command,
    endpoint: SocketAddrV4,
    checks: &[LoopbackHttpCheck],
    decisions: &[PolicyDecisionId],
    phase: LoopbackStartupPhase,
    runtime: &tokio::runtime::Runtime,
    effect_end: Instant,
    end: Instant,
    control: &dyn ProcessRunControl,
) -> Stage<Vec<LoopbackHttpObservation>> {
    let ready = (|| {
        check_revision(project, prepared.script(), control)?;
        if !observe_until(effect_end, control, |time| {
            platform::listener_absent(endpoint, time)
        })? {
            return Err(Failure::Occupied);
        }
        remaining(effect_end, control)?;
        Ok(())
    })();
    if let Err(failure) = ready {
        return Stage::Clean(Err(failure));
    }
    let mut child = match start_borrowed_owned(command) {
        Ok(child) => child,
        Err(Outcome::NotStarted(failure)) => return Stage::Clean(Err(failure)),
        Err(_) => return Stage::Unknown,
    };
    let mut mismatch = None;
    let result = (|| {
        let mut observations = Vec::with_capacity(checks.len());
        for (index, (check, decision)) in checks.iter().zip(decisions).enumerate() {
            let stream = owned_stream(&child, endpoint, effect_end, control)?;
            let observation = runtime.block_on(http::probe(
                stream,
                endpoint,
                check.probe(),
                *decision,
                remaining(effect_end, control)?,
                control,
            ))?;
            remaining(effect_end, control)?;
            if !check.matches(&observation) {
                let original_index = if phase == LoopbackStartupPhase::Default {
                    prepared
                        .recipe()
                        .default_start_check_index()
                        .ok_or(Failure::Denied)?
                } else {
                    u8::try_from(index).map_err(|_| Failure::Denied)?
                };
                mismatch = Some(
                    a3_domain::LoopbackHttpMismatch::new(
                        prepared.recipe(),
                        phase,
                        original_index,
                        observation,
                    )
                    .map_err(|_| Failure::Denied)?,
                );
                return Err(Failure::InvalidResponse);
            }
            observations.push(observation);
        }
        wait_owned(&child, endpoint, None, effect_end, control)?;
        Ok(observations)
    })();
    if stop(&mut child, endpoint, end).is_none() {
        return Stage::Unknown;
    }
    let current = check_revision(project, prepared.script(), control)
        .and_then(|()| remaining(end, control).map(|_| ()));
    match (current, mismatch) {
        (Ok(()), Some(mismatch)) => Stage::Mismatched(mismatch),
        (current, _) => Stage::Clean(current.and(result)),
    }
}

fn observe_failure_exit(
    child: &LoopbackChild,
    effect_end: Instant,
    control: &dyn ProcessRunControl,
) -> Stage<LoopbackFailureExitCode> {
    loop {
        let time = match remaining(effect_end, control) {
            Ok(time) => time,
            Err(failure) => return Stage::Clean(Err(failure)),
        };
        #[cfg(target_os = "linux")]
        match platform::poll_exit_before_stop(child.id()) {
            Ok(Some(platform::LinuxObservedExit::Code(code))) => {
                return Stage::Clean(remaining(effect_end, control).and_then(|_| {
                    LoopbackFailureExitCode::new(code).map_err(|_| Failure::Exited)
                }));
            }
            Ok(Some(platform::LinuxObservedExit::Signal)) => {
                return Stage::Clean(Err(Failure::Exited));
            }
            Ok(Some(platform::LinuxObservedExit::OwnerLost)) | Err(_) => return Stage::Unknown,
            Ok(None) => {}
        }
        #[cfg(windows)]
        match observe_until(effect_end, control, |_| child.poll_exit_before_stop()) {
            Ok(Some(platform::WindowsObservedExit::ApplicationFailure(code))) => {
                return Stage::Clean(remaining(effect_end, control).and_then(|_| {
                    LoopbackFailureExitCode::new(u32::from(code.get())).map_err(|_| Failure::Exited)
                }));
            }
            Ok(Some(
                platform::WindowsObservedExit::Zero | platform::WindowsObservedExit::OutOfRange(_),
            )) => {
                return Stage::Clean(Err(Failure::Exited));
            }
            Err(failure) => return Stage::Clean(Err(failure)),
            Ok(None) => {}
        }
        if control.wait_cancelled_timeout(time.min(POLL)) {
            return Stage::Clean(Err(Failure::Cancelled));
        }
    }
}

fn hold_until(end: Instant) {
    while let Some(time) = time_until(end) {
        thread::sleep(time.min(POLL));
    }
}

fn stopped(result: Result<LoopbackStartupVerificationReport, Failure>) -> StartupOutcome {
    StartupOutcome::Stopped {
        cleanup: if cfg!(windows) {
            LoopbackStartupCleanupProof::WindowsJobsAndEndpoints
        } else {
            LoopbackStartupCleanupProof::ProcessGroupsAndEndpoints
        },
        result: result.map(Box::new),
    }
}

fn mismatched(mismatch: a3_domain::LoopbackHttpMismatch) -> StartupOutcome {
    StartupOutcome::Mismatched {
        cleanup: if cfg!(windows) {
            LoopbackStartupCleanupProof::WindowsJobsAndEndpoints
        } else {
            LoopbackStartupCleanupProof::ProcessGroupsAndEndpoints
        },
        mismatch,
    }
}
