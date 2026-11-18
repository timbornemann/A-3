//! Failed HTTP observations remain bounded diagnoses, never success evidence.
use a3_domain::*;
use std::{error::Error, num::NonZeroU16};
type TestResult = Result<(), Box<dyn Error>>;
fn recipe() -> Result<LoopbackHttpRecipe, Box<dyn Error>> {
    Ok(LoopbackHttpRecipe::new(
        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
        LoopbackStart::Explicit,
        NonZeroU16::try_from(18765)?,
        vec![
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/health".to_owned())?,
                200,
                LoopbackExpectedBody::Exact {
                    hash: ContentHash::from_bytes([1; 32]),
                    bytes: 6,
                },
                LoopbackHeaderChecks::default(),
            )?,
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/missing".to_owned())?,
                404,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks::default(),
            )?,
        ],
    )?
    .with_startup_coverage(0)?)
}
fn observed(decision: u8, body: u8) -> Result<LoopbackHttpObservation, Box<dyn Error>> {
    Ok(LoopbackHttpObservation::new(
        PolicyDecisionId::from_bytes([decision; 32]),
        200,
        ContentHash::from_bytes([body; 32]),
        6,
        LoopbackHeaderChecks::default(),
    )?)
}
fn context() -> LoopbackHttpEvidenceContext {
    LoopbackHttpEvidenceContext {
        verification_run_id: VerificationRunId::from_bytes([20; 32]),
        spec_id: VerificationSpecId::from_bytes([21; 32]),
        run_id: AgentRunId::from_bytes([22; 32]),
        tool_run_id: ToolRunId::from_bytes([23; 32]),
        step_id: TaskStepId::from_bytes([24; 32]),
        snapshot_id: SnapshotId::from_bytes([25; 32]),
    }
}
#[test]
fn mismatch_requires_real_difference_and_original_http_phase_position() -> TestResult {
    let r = recipe()?;
    for phase in [
        LoopbackStartupPhase::ReserveEndpoint,
        LoopbackStartupPhase::HeldExplicit,
        LoopbackStartupPhase::InvalidPort,
    ] {
        assert!(LoopbackHttpMismatch::new(&r, phase, 0, observed(3, 2)?).is_err());
    }
    assert!(
        LoopbackHttpMismatch::new(&r, LoopbackStartupPhase::FreeExplicit, 2, observed(3, 2)?)
            .is_err()
    );
    assert!(
        LoopbackHttpMismatch::new(&r, LoopbackStartupPhase::Default, 1, observed(6, 2)?).is_err()
    );
    assert!(
        LoopbackHttpMismatch::new(&r, LoopbackStartupPhase::FreeExplicit, 0, observed(3, 1)?)
            .is_err()
    );
    for phase in [
        LoopbackStartupPhase::FreeExplicit,
        LoopbackStartupPhase::Default,
    ] {
        let m = LoopbackHttpMismatch::new(&r, phase, 0, observed(3, 2)?)?;
        assert_eq!(m.recipe_id(), r.id());
        assert_eq!(m.check_index(), 0);
        assert_eq!(m.phase(), phase);
    }
    Ok(())
}
#[test]
fn diagnostic_binds_process_scope_all_decisions_and_digest() -> TestResult {
    let r = recipe()?;
    let c = context();
    let worktree = WorktreeId::from_bytes([26; 32]);
    let limit = ProcessOutputLimit::new(1024)?;
    let settings = LoopbackProcessSettings {
        timeout: ProcessTimeout::from_millis(8000)?,
        stdout_limit: limit,
        stderr_limit: limit,
        system_root: false,
    };
    let process =
        LoopbackHttpProcess::from_spec(&r.process_spec(c.run_id, worktree, c.step_id, settings)?)?;
    let script = FileRevision::new(r.script().clone(), ContentHash::from_bytes([27; 32]));
    let decisions: Vec<_> = (0..8)
        .map(|v| PolicyDecisionId::from_bytes([v; 32]))
        .collect();
    let m = LoopbackHttpMismatch::new(&r, LoopbackStartupPhase::FreeExplicit, 0, observed(3, 2)?)?;
    let make = |ctx, process, script, decisions| {
        LoopbackHttpMismatchDiagnostic::new(
            ctx,
            AgentToolAttemptNumber::FIRST,
            worktree,
            &r,
            script,
            process,
            m,
            decisions,
        )
    };
    let d = make(c, process, script.clone(), decisions.clone())?;
    assert_eq!(d.digest(), d.clone().digest());
    let mut foreign = c;
    foreign.run_id = AgentRunId::from_bytes([99; 32]);
    assert!(make(foreign, process, script.clone(), decisions.clone()).is_err());
    assert!(
        make(
            c,
            LoopbackHttpProcess::new(PolicyResourceId::from_bytes([99; 32]), settings),
            script.clone(),
            decisions.clone()
        )
        .is_err()
    );
    assert!(
        make(
            c,
            process,
            FileRevision::new(
                RepositoryPath::try_from_bytes(b"other.py".to_vec())?,
                script.content_hash()
            ),
            decisions.clone()
        )
        .is_err()
    );
    let mut swapped = decisions.clone();
    swapped.swap(2, 3);
    assert!(make(c, process, script.clone(), swapped).is_err());
    let mut repeated = decisions.clone();
    repeated[7] = repeated[0];
    assert!(make(c, process, script.clone(), repeated).is_err());
    assert!(make(c, process, script.clone(), decisions[..7].to_vec()).is_err());
    let changed = FileRevision::new(r.script().clone(), ContentHash::from_bytes([28; 32]));
    assert_ne!(d.digest(), make(c, process, changed, decisions)?.digest());
    assert!(AgentToolAttemptNumber::new(0).is_err());
    Ok(())
}
