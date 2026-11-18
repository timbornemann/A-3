//! Controlled verifier state: a completed local diff is not a final runtime criterion proof.
use super::*;
use futures::executor::block_on;
#[test]
fn completed_progress_diff_keeps_runtime_must_open_before_global_evidence_reads()
-> Result<(), Box<dyn Error>> {
    let spec = VerificationSpec::diff_invariant(
        spec_id(112),
        requirement()?,
        DiffInvariantVerification::new(DiffInvariantMode::NonEmptyChanges, Vec::new())?,
    );
    let (request, store) = original_fixture(
        SourceCase::ProgressOnly,
        spec,
        passing_original_case()?,
        None,
    )?;
    let step = request.task_ledger().steps().next().ok_or("step")?;
    assert_eq!(step.status(), a3_domain::TaskStepStatus::Completed);
    assert!(!step.definition().addressed_criteria().is_empty());
    assert!(step.definition().acceptance_criteria().is_empty());
    assert_eq!(
        VerificationEvidenceEvaluation::evaluate(
            step.definition().verification_spec(),
            &store.state.evidence()[0]
        ),
        VerificationEvidenceEvaluation::Passed
    );
    assert!(
        crate::AdmitSourceTaskStep
            .execute(request.goal_contract(), step.definition())
            .is_ok()
    );
    assert!(required_acceptance_evidence(&request)?.is_empty());
    assert!(matches!(
        block_on(DeterministicAcceptanceVerifier::new(&store).verify(
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl
        ))?,
        AcceptanceVerifierOutcome::Rejected(AcceptanceRejection::InsufficientEvidence)
    ));
    assert_eq!(store.reads.load(Ordering::SeqCst), 0);
    Ok(())
}
#[test]
fn final_matching_test_proves_without_importing_progress_diff_as_runtime_evidence()
-> Result<(), Box<dyn Error>> {
    let diff = VerificationSpec::diff_invariant(
        spec_id(113),
        requirement()?,
        DiffInvariantVerification::new(DiffInvariantMode::NonEmptyChanges, Vec::new())?,
    );
    let (request, store) = original_fixture(
        SourceCase::Complete,
        original_test_spec()?,
        passing_original_case()?,
        Some(diff),
    )?;
    let proof = request
        .task_ledger()
        .steps()
        .find(|step| !step.definition().acceptance_criteria().is_empty())
        .ok_or("proof step")?;
    let progress = request
        .task_ledger()
        .steps()
        .find(|step| !step.definition().addressed_criteria().is_empty())
        .ok_or("progress step")?;
    assert!(progress.definition().acceptance_criteria().is_empty());
    let required = required_acceptance_evidence(&request)?;
    assert_eq!(required.len(), 1);
    assert_eq!(required[0].1.len(), 1);
    assert_eq!(
        required[0].1[0].0.id(),
        proof.definition().verification_spec().id()
    );
    assert!(matches!(
        block_on(DeterministicAcceptanceVerifier::new(&store).verify(
            &request,
            AcceptanceVerifierTimeout::DEFAULT,
            &ActiveControl
        ))?,
        AcceptanceVerifierOutcome::Accepted(_)
    ));
    assert_eq!(store.reads.load(Ordering::SeqCst), 1);
    Ok(())
}
