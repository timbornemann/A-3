//! Exact authorization contracts for a complete local startup verification.

use a3_application::{
    AuthorizedLoopbackStartupCoverage, LoopbackVerificationFailure, PreparedLoopbackStartupCoverage,
};
use a3_domain::{
    ActionClass, AgentPermissionMode, AgentPermissionRevision, AgentPermissionSettings, AgentRunId,
    AgentRunTimestamp, ApprovalGrant, ApprovalId, ApprovalRequest, ApprovalRequestId, ContentHash,
    FileRevision, LoopbackExpectedBody, LoopbackFailureExitCode, LoopbackHeaderChecks,
    LoopbackHttpCheck, LoopbackHttpRecipe, LoopbackMethod, LoopbackProbe, LoopbackProcessSettings,
    LoopbackStart, LoopbackStartupPhase, PolicyAction, PolicyDecision, PolicyDecisionId,
    PolicyDisposition, PolicyEvaluationTiming, ProcessOutputLimit, ProcessTimeout, RepositoryPath,
    SnapshotId, SystemPolicyV1, TaskStepId, WorktreeId,
};
use std::{error::Error, num::NonZeroU16};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const RUN: AgentRunId = AgentRunId::from_bytes([1; 32]);
const WORKTREE: WorktreeId = WorktreeId::from_bytes([2; 32]);
const STEP: TaskStepId = TaskStepId::from_bytes([3; 32]);

#[test]
fn failure_exit_codes_admit_only_the_closed_application_error_range() -> TestResult {
    for value in [0, 256, 259, 0xc000_0005, u32::MAX] {
        assert!(LoopbackFailureExitCode::new(value).is_err());
    }
    for value in [1, 255] {
        assert_eq!(u32::from(LoopbackFailureExitCode::new(value)?.get()), value);
    }
    Ok(())
}

#[test]
fn held_and_free_starts_share_argv_but_require_distinct_exact_authority() -> TestResult {
    let prepared = prepared(4, 5, 18_765)?;
    let held = prepared
        .process(LoopbackStartupPhase::HeldExplicit)
        .ok_or("missing held start")?;
    let free = prepared
        .process(LoopbackStartupPhase::FreeExplicit)
        .ok_or("missing free start")?;
    assert_eq!(held, free);
    assert_eq!(held.arguments(), free.arguments());
    assert_eq!(held.specification_id(), free.specification_id());

    let actions = prepared.policy_actions();
    assert_startup_phase(&actions[0], LoopbackStartupPhase::ReserveEndpoint);
    assert_startup_phase(&actions[1], LoopbackStartupPhase::HeldExplicit);
    assert_startup_phase(&actions[2], LoopbackStartupPhase::FreeExplicit);
    assert_ne!(actions[1].fingerprint(), actions[2].fingerprint());
    assert_ne!(actions[1].scope_digest(), actions[2].scope_digest());

    let timing = times()?;
    let request = ApprovalRequest::new(
        ApprovalRequestId::from_bytes([20; 32]),
        RUN,
        &actions[1],
        timing.0,
        timing.1,
    )?;
    let mut grant = ApprovalGrant::grant(ApprovalId::from_bytes([21; 32]), &request, timing.0)?;
    assert!(
        grant
            .consume(
                PolicyDecisionId::from_bytes([22; 32]),
                RUN,
                &actions[2],
                timing.0
            )
            .is_err()
    );
    grant.consume(
        PolicyDecisionId::from_bytes([23; 32]),
        RUN,
        &actions[1],
        timing.0,
    )?;
    Ok(())
}

#[test]
fn endpoint_reservation_requires_exact_approval_in_both_permission_modes() -> TestResult {
    let expected = prepared(4, 5, 18_765)?;
    let other_port = prepared(4, 5, 18_766)?;
    let actions = expected.policy_actions();
    let other_actions = other_port.policy_actions();
    let reserve = &actions[0];
    assert_eq!(reserve.class(), ActionClass::Network);
    assert_ne!(reserve.fingerprint(), other_actions[0].fingerprint());
    assert_ne!(reserve.scope_digest(), other_actions[0].scope_digest());

    for mode in [
        AgentPermissionMode::AskPermissions,
        AgentPermissionMode::FullMachine,
    ] {
        let settings = AgentPermissionSettings::new(mode, AgentPermissionRevision::new(2)?);
        for action in &actions {
            assert_eq!(
                settings.disposition(action),
                PolicyDisposition::ApprovalRequired
            );
            assert_eq!(
                SystemPolicyV1.disposition(action),
                PolicyDisposition::ApprovalRequired
            );
        }
    }
    let (now, expires) = times()?;
    let request = ApprovalRequest::new(
        ApprovalRequestId::from_bytes([24; 32]),
        RUN,
        reserve,
        now,
        expires,
    )?;
    let mut grant = ApprovalGrant::grant(ApprovalId::from_bytes([25; 32]), &request, now)?;
    assert!(
        grant
            .consume(
                PolicyDecisionId::from_bytes([26; 32]),
                RUN,
                &other_actions[0],
                now
            )
            .is_err()
    );
    grant.consume(PolicyDecisionId::from_bytes([27; 32]), RUN, reserve, now)?;
    Ok(())
}

#[test]
fn startup_authorization_requires_every_distinct_decision_in_runtime_order() -> TestResult {
    let expected = prepared(4, 5, 18_765)?;
    let actions = expected.policy_actions();
    assert_eq!(actions.len(), expected.recipe().checks().len() + 6);
    for (index, method) in [
        LoopbackMethod::Get,
        LoopbackMethod::Get,
        LoopbackMethod::Post,
        LoopbackMethod::Put,
        LoopbackMethod::Delete,
        LoopbackMethod::Patch,
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            matches!(actions[3 + index], PolicyAction::LoopbackHttpProbe { method: actual, .. } if actual == method)
        );
    }
    let default_index = 3 + expected.recipe().checks().len();
    assert_startup_phase(&actions[default_index], LoopbackStartupPhase::Default);
    assert!(matches!(
        actions[default_index + 1],
        PolicyAction::LoopbackHttpProbe {
            method: LoopbackMethod::Get,
            ..
        }
    ));
    assert_startup_phase(
        &actions[default_index + 2],
        LoopbackStartupPhase::InvalidPort,
    );

    let approvals = approve_all(&actions, RUN)?;
    let authorized = AuthorizedLoopbackStartupCoverage::new(prepared(4, 5, 18_765)?, &approvals)?;
    let (actual, admitted) = authorized.into_parts();
    assert_eq!(actual, expected);
    assert_eq!(
        admitted.ordered(),
        approvals.iter().map(PolicyDecision::id).collect::<Vec<_>>()
    );
    assert_eq!(
        admitted.explicit_probes.len(),
        expected.recipe().checks().len()
    );

    assert_denied(prepared(4, 5, 18_765)?, &approvals[..approvals.len() - 1]);
    let mut extra = approvals.clone();
    extra.push(approvals[0].clone());
    assert_denied(prepared(4, 5, 18_765)?, &extra);

    for pair in [(0, 1), (1, 2), (3, 4), (default_index, default_index + 1)] {
        let mut reordered = approvals.clone();
        reordered.swap(pair.0, pair.1);
        assert_denied(prepared(4, 5, 18_765)?, &reordered);
    }
    let mut repeated_id = approvals.clone();
    repeated_id[1] = approve(&actions[1], RUN, 95, approvals[0].id())?;
    assert_denied(prepared(4, 5, 18_765)?, &repeated_id);

    let wrong_run = approve_all(&actions, AgentRunId::from_bytes([99; 32]))?;
    assert_denied(prepared(4, 5, 18_765)?, &wrong_run);
    let mut automatic = approvals.clone();
    automatic[0] = PolicyDecision::automatic(
        approvals[0].id(),
        RUN,
        &actions[0],
        PolicyEvaluationTiming::new(times()?.0, times()?.0)?,
    );
    assert_denied(prepared(4, 5, 18_765)?, &automatic);
    Ok(())
}

#[test]
fn compound_effect_and_approvals_bind_snapshot_script_and_exact_endpoint() -> TestResult {
    let expected = prepared(4, 5, 18_765)?;
    let approvals = approve_all(&expected.policy_actions(), RUN)?;
    let repeated = prepared(4, 5, 18_765)?;
    assert_eq!(expected.effect_resource(), repeated.effect_resource());
    assert_eq!(expected.effect_fingerprint(), repeated.effect_fingerprint());
    for changed in [
        prepared(6, 5, 18_765)?,
        prepared(4, 7, 18_765)?,
        prepared(4, 5, 18_766)?,
    ] {
        assert_ne!(expected.effect_resource(), changed.effect_resource());
        assert_ne!(expected.effect_fingerprint(), changed.effect_fingerprint());
        assert_denied(changed, &approvals);
    }
    let wrong_path = FileRevision::new(
        RepositoryPath::try_from_bytes(b"other.py".to_vec())?,
        hash(5),
    );
    assert!(matches!(
        PreparedLoopbackStartupCoverage::new(
            RUN,
            WORKTREE,
            STEP,
            SnapshotId::from_bytes([4; 32]),
            wrong_path,
            recipe(18_765)?.with_startup_coverage(0)?,
            settings()?,
        ),
        Err(LoopbackVerificationFailure::Denied)
    ));
    assert!(matches!(
        PreparedLoopbackStartupCoverage::new(
            RUN,
            WORKTREE,
            STEP,
            SnapshotId::from_bytes([4; 32]),
            script(5)?,
            recipe(18_765)?,
            settings()?,
        ),
        Err(LoopbackVerificationFailure::Denied)
    ));
    Ok(())
}

fn prepared(snapshot: u8, source: u8, port: u16) -> TestResult<PreparedLoopbackStartupCoverage> {
    Ok(PreparedLoopbackStartupCoverage::new(
        RUN,
        WORKTREE,
        STEP,
        SnapshotId::from_bytes([snapshot; 32]),
        script(source)?,
        recipe(port)?.with_startup_coverage(0)?,
        settings()?,
    )?)
}

fn settings() -> TestResult<LoopbackProcessSettings> {
    let limit = ProcessOutputLimit::new(1024)?;
    Ok(LoopbackProcessSettings {
        timeout: ProcessTimeout::from_millis(8_000)?,
        stdout_limit: limit,
        stderr_limit: limit,
        system_root: false,
    })
}

fn script(value: u8) -> TestResult<FileRevision> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
        hash(value),
    ))
}

fn hash(value: u8) -> ContentHash {
    ContentHash::from_bytes([value; 32])
}

fn recipe(port: u16) -> TestResult<LoopbackHttpRecipe> {
    let mut checks = vec![
        LoopbackHttpCheck::new(
            LoopbackProbe::new(LoopbackMethod::Get, "/health".to_owned())?,
            200,
            LoopbackExpectedBody::Exact {
                hash: hash(8),
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
    Ok(LoopbackHttpRecipe::new(
        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
        LoopbackStart::Explicit,
        NonZeroU16::try_from(port)?,
        checks,
    )?)
}

fn assert_startup_phase(action: &PolicyAction, expected: LoopbackStartupPhase) {
    assert!(matches!(action, PolicyAction::LoopbackStartup { phase, .. } if *phase == expected));
}

fn assert_denied(prepared: PreparedLoopbackStartupCoverage, decisions: &[PolicyDecision]) {
    assert!(matches!(
        AuthorizedLoopbackStartupCoverage::new(prepared, decisions),
        Err(LoopbackVerificationFailure::Denied)
    ));
}

fn times() -> TestResult<(AgentRunTimestamp, AgentRunTimestamp)> {
    Ok((
        AgentRunTimestamp::from_unix_millis(10)?,
        AgentRunTimestamp::from_unix_millis(100)?,
    ))
}

fn approve_all(actions: &[PolicyAction], run: AgentRunId) -> TestResult<Vec<PolicyDecision>> {
    actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let seed = u8::try_from(index + 32)?;
            approve(action, run, seed, PolicyDecisionId::from_bytes([seed; 32]))
        })
        .collect()
}

fn approve(
    action: &PolicyAction,
    run: AgentRunId,
    seed: u8,
    decision_id: PolicyDecisionId,
) -> TestResult<PolicyDecision> {
    let (now, expires) = times()?;
    let request = ApprovalRequest::new(
        ApprovalRequestId::from_bytes([seed; 32]),
        run,
        action,
        now,
        expires,
    )?;
    let mut grant = ApprovalGrant::grant(ApprovalId::from_bytes([seed; 32]), &request, now)?;
    grant.consume(decision_id, run, action, now)?;
    Ok(PolicyDecision::approved(
        decision_id,
        run,
        action,
        &grant,
        PolicyEvaluationTiming::new(now, now)?,
    )?)
}
