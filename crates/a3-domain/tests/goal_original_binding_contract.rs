//! Original-backed goals cannot lose source or criterion provenance across revisions.

use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
    AcceptanceCriterionStatement, CriterionEvidenceRequirement, CriterionOrigin, GoalContract,
    GoalContractDraft, GoalContractHistory, GoalContractHistoryError, GoalContractReference,
    GoalContractRevision, GoalContractRevisionFailure, GoalContractTimestamp, GoalObjective,
    GoalOriginalBindingError, GoalOriginalError, GoalOriginalRequest, GoalRevisionReason,
    SuccessVerification, TaskId,
};
use std::error::Error;

fn criterion(id: u8) -> Result<AcceptanceCriterion, Box<dyn Error>> {
    Ok(AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([id; 32]),
        AcceptanceCriterionStatement::try_from_string(format!("required outcome {id}"))?,
    ))
}

fn draft(
    objective: &str,
    criteria: Vec<AcceptanceCriterion>,
) -> Result<GoalContractDraft, Box<dyn Error>> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string(objective.to_owned())?,
        criteria,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("verify requested outcomes".to_owned())?,
    )?)
}

fn legacy(task: u8) -> Result<GoalContract, Box<dyn Error>> {
    Ok(GoalContract::initial(
        TaskId::from_bytes([task; 32]),
        draft("initial goal", vec![criterion(1)?])?,
        GoalContractTimestamp::from_unix_millis(1)?,
    ))
}

fn original(
    goal: GoalContractReference,
    text: &str,
) -> Result<GoalOriginalRequest, Box<dyn Error>> {
    Ok(GoalOriginalRequest::try_from_bytes(
        goal,
        text.as_bytes().to_vec(),
    )?)
}

fn bound_draft(original: &GoalOriginalRequest) -> Result<GoalContractDraft, Box<dyn Error>> {
    let origin = CriterionOrigin::new(
        original.range(0, original.as_bytes().len())?,
        CriterionEvidenceRequirement::RuntimeNeedsRecipe,
    );
    draft("grounded goal", vec![criterion(1)?.with_origin(origin)])
}

fn sourced(task: u8) -> Result<GoalContract, Box<dyn Error>> {
    let prototype = legacy(task)?;
    let source = original(prototype.reference(), "R01: keep the complete original\r\n")?;
    Ok(GoalContract::initial(
        prototype.task_id(),
        bound_draft(&source)?,
        prototype.created_at(),
    )
    .with_original(source)?)
}

#[test]
fn legacy_constructors_do_not_fabricate_an_original_from_the_objective()
-> Result<(), Box<dyn Error>> {
    let initial = legacy(1)?;
    assert!(initial.original().is_none());
    assert_eq!(initial.validate_original_binding(), Ok(()));
    let restored = GoalContract::reconstruct(
        initial.task_id(),
        initial.revision(),
        None,
        None,
        initial.draft().clone(),
        initial.created_at(),
    )?;
    assert!(restored.original().is_none());
    assert_eq!(restored.validate_original_binding(), Ok(()));
    Ok(())
}

#[test]
fn criterion_origin_without_an_original_is_not_a_valid_legacy_goal() -> Result<(), Box<dyn Error>> {
    let source = sourced(1)?;
    let orphan = GoalContract::initial(
        source.task_id(),
        source.draft().clone(),
        source.created_at(),
    );
    assert_eq!(
        orphan.validate_original_binding(),
        Err(GoalOriginalBindingError::MissingOriginal)
    );
    assert_eq!(
        GoalContractHistory::new(vec![orphan]),
        Err(GoalContractHistoryError::InvalidOriginalBinding(
            GoalOriginalBindingError::MissingOriginal
        ))
    );
    Ok(())
}

#[test]
fn source_owner_and_every_criterion_including_should_are_checked() -> Result<(), Box<dyn Error>> {
    let base = legacy(1)?;
    let foreign = original(legacy(2)?.reference(), "R01: original\r\n")?;
    assert_eq!(
        base.clone().with_original(foreign),
        Err(GoalOriginalBindingError::OriginalGoalMismatch)
    );
    let source = original(base.reference(), "R01: original\r\n")?;
    let origin = CriterionOrigin::new(
        source.range(0, source.as_bytes().len())?,
        CriterionEvidenceRequirement::RuntimeNeedsRecipe,
    );
    let missing = AcceptanceCriterion::with_requirement(
        AcceptanceCriterionId::from_bytes([2; 32]),
        AcceptanceCriterionStatement::try_from_string("advisory outcome".to_owned())?,
        AcceptanceCriterionRequirement::Should,
    );
    let candidate = GoalContract::initial(
        base.task_id(),
        draft(
            "grounded goal",
            vec![criterion(1)?.with_origin(origin), missing],
        )?,
        base.created_at(),
    );
    assert_eq!(
        candidate.with_original(source),
        Err(GoalOriginalBindingError::MissingCriterionOrigin {
            criterion_id: AcceptanceCriterionId::from_bytes([2; 32])
        })
    );
    Ok(())
}

#[test]
fn foreign_and_stale_criterion_ranges_do_not_attach_to_current_original()
-> Result<(), Box<dyn Error>> {
    let base = legacy(1)?;
    let source = original(base.reference(), "R01: current original\r\n")?;
    for (other, error) in [
        (
            original(legacy(2)?.reference(), source.as_str())?,
            GoalOriginalError::GoalMismatch,
        ),
        (
            original(base.reference(), "R01: older original\r\n")?,
            GoalOriginalError::DigestMismatch,
        ),
    ] {
        let criterion = criterion(1)?.with_origin(CriterionOrigin::new(
            other.range(0, other.as_bytes().len())?,
            CriterionEvidenceRequirement::RuntimeNeedsRecipe,
        ));
        let candidate = GoalContract::initial(
            base.task_id(),
            draft("grounded goal", vec![criterion])?,
            base.created_at(),
        );
        assert_eq!(
            candidate.with_original(source.clone()),
            Err(GoalOriginalBindingError::InvalidCriterionOrigin {
                criterion_id: AcceptanceCriterionId::from_bytes([1; 32]),
                error
            })
        );
    }
    Ok(())
}

#[test]
fn original_backed_revision_requires_an_explicit_next_revision_original()
-> Result<(), Box<dyn Error>> {
    let initial = sourced(1)?;
    let changed = draft("updated outcome", vec![criterion(1)?])?;
    let reason = GoalRevisionReason::try_from_string("user changed the request".to_owned())?;
    let time = GoalContractTimestamp::from_unix_millis(2)?;
    assert_eq!(
        initial.revise(changed, reason.clone(), time),
        Err(GoalContractRevisionFailure::OriginalRequired)
    );
    let next_ref = initial.next_reference()?;
    assert_eq!(next_ref.task_id(), initial.task_id());
    assert_eq!(next_ref.revision().get(), 2);
    let next_source = original(next_ref, "R01: new exact request\r\n")?;
    let next = initial.revise_with_original(
        bound_draft(&next_source)?,
        reason,
        time,
        next_source.clone(),
    )?;
    assert_eq!(next.reference(), next_ref);
    assert_eq!(next.original(), Some(&next_source));
    assert_eq!(next.validate_original_binding(), Ok(()));
    assert_ne!(initial.original(), next.original());
    let mut history = GoalContractHistory::new(vec![initial.clone()])?;
    history.append(next.clone())?;
    assert_eq!(history.revisions(), &[initial, next]);
    Ok(())
}

#[test]
fn original_is_never_rebound_automatically_to_the_next_revision() -> Result<(), Box<dyn Error>> {
    let initial = sourced(1)?;
    let old = initial.original().ok_or("missing test original")?.clone();
    let source = original(initial.next_reference()?, "R01: new exact request\r\n")?;
    assert_eq!(
        initial.revise_with_original(
            bound_draft(&source)?,
            GoalRevisionReason::try_from_string("changed".to_owned())?,
            GoalContractTimestamp::from_unix_millis(2)?,
            old
        ),
        Err(GoalContractRevisionFailure::InvalidOriginalBinding(
            GoalOriginalBindingError::OriginalGoalMismatch
        ))
    );
    Ok(())
}

#[test]
fn legacy_reconstruct_cannot_drop_source_through_history_new_or_append()
-> Result<(), Box<dyn Error>> {
    let initial = sourced(1)?;
    let legacy_successor = GoalContract::reconstruct(
        initial.task_id(),
        GoalContractRevision::new(2)?,
        Some(initial.revision()),
        Some(GoalRevisionReason::try_from_string(
            "claimed change".to_owned(),
        )?),
        draft("changed legacy objective", vec![criterion(1)?])?,
        GoalContractTimestamp::from_unix_millis(2)?,
    )?;
    assert_eq!(legacy_successor.validate_original_binding(), Ok(()));
    assert_eq!(
        GoalContractHistory::new(vec![initial.clone(), legacy_successor.clone()]),
        Err(GoalContractHistoryError::OriginalRequired)
    );
    let mut history = GoalContractHistory::new(vec![initial.clone()])?;
    assert_eq!(
        history.append(legacy_successor),
        Err(GoalContractHistoryError::OriginalRequired)
    );
    assert_eq!(history.revisions(), &[initial]);
    Ok(())
}

#[test]
fn source_less_legacy_revisions_and_histories_keep_their_existing_behavior()
-> Result<(), Box<dyn Error>> {
    let initial = legacy(1)?;
    let next = initial.revise(
        draft("changed legacy objective", vec![criterion(1)?])?,
        GoalRevisionReason::try_from_string("user changed goal".to_owned())?,
        GoalContractTimestamp::from_unix_millis(2)?,
    )?;
    assert!(next.original().is_none());
    assert_eq!(
        GoalContractHistory::new(vec![initial.clone(), next.clone()])?.revisions(),
        &[initial, next]
    );
    Ok(())
}

#[test]
fn next_reference_uses_checked_revision_arithmetic() -> Result<(), Box<dyn Error>> {
    let initial = legacy(1)?;
    let last = GoalContract::reconstruct(
        initial.task_id(),
        GoalContractRevision::new(u32::MAX)?,
        Some(GoalContractRevision::new(u32::MAX - 1)?),
        Some(GoalRevisionReason::try_from_string(
            "last revision".to_owned(),
        )?),
        initial.draft().clone(),
        initial.created_at(),
    )?;
    assert_eq!(
        last.next_reference(),
        Err(GoalContractRevisionFailure::RevisionExhausted)
    );
    Ok(())
}

#[test]
fn binding_errors_and_original_backed_goal_debug_remain_content_free() -> Result<(), Box<dyn Error>>
{
    let initial = legacy(1)?;
    let marker = "private-original-marker";
    let source = original(initial.reference(), marker)?;
    let goal = GoalContract::initial(
        initial.task_id(),
        bound_draft(&source)?,
        initial.created_at(),
    )
    .with_original(source)?;
    assert!(!format!("{goal:?}").contains(marker));
    for error in [
        GoalOriginalBindingError::MissingOriginal,
        GoalOriginalBindingError::OriginalGoalMismatch,
        GoalOriginalBindingError::MissingCriterionOrigin {
            criterion_id: AcceptanceCriterionId::from_bytes([1; 32]),
        },
        GoalOriginalBindingError::InvalidCriterionOrigin {
            criterion_id: AcceptanceCriterionId::from_bytes([1; 32]),
            error: GoalOriginalError::DigestMismatch,
        },
    ] {
        assert!(!format!("{error} {error:?}").contains(marker));
    }
    Ok(())
}
