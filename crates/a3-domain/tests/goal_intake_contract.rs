//! Pending is exact non-executable capture; only a complete initial proposal can consume it.
use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
    CriterionEvidenceRequirement, CriterionOrigin, DiscoveredCommandId, GoalConstraint,
    GoalContract, GoalContractDraft, GoalContractReference, GoalContractRevision,
    GoalContractTimestamp, GoalIntakeError, GoalObjective, GoalOriginalCoverage,
    GoalOriginalRequest, GoalRevisionReason, MAX_GOAL_ORIGINAL_BYTES, MinimumTestCaseCount,
    PendingGoalIntake, SuccessVerification, TaskId, TestCaseSelector, VerificationScope,
};
use std::error::Error;

fn at(value: u64) -> Result<GoalContractTimestamp, Box<dyn Error>> {
    Ok(GoalContractTimestamp::from_unix_millis(value)?)
}
fn pending(task: u8, text: &str) -> Result<PendingGoalIntake, Box<dyn Error>> {
    Ok(PendingGoalIntake::new(
        GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(TaskId::from_bytes([task; 32])),
            text.as_bytes().to_vec(),
        )?,
        at(1)?,
    )?)
}
fn criterion() -> Result<AcceptanceCriterion, Box<dyn Error>> {
    Ok(AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([9; 32]),
        AcceptanceCriterionStatement::try_from_string(
            "requested behavior is implemented".to_owned(),
        )?,
    ))
}
fn draft(
    criteria: Vec<AcceptanceCriterion>,
    constraints: Vec<GoalConstraint>,
) -> Result<GoalContractDraft, Box<dyn Error>> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("Implement requested behavior".to_owned())?,
        criteria,
        constraints,
        Vec::new(),
        Vec::new(),
        SuccessVerification::try_from_string("the matching test passes".to_owned())?,
    )?)
}
fn proposed(
    reference: GoalContractReference,
    text: &str,
    created_at: GoalContractTimestamp,
    end: usize,
    unbound: bool,
) -> Result<GoalContract, Box<dyn Error>> {
    let original = GoalOriginalRequest::try_from_bytes(reference, text.as_bytes().to_vec())?;
    let proof = CriterionEvidenceRequirement::ExistingTest {
        command: DiscoveredCommandId::from_bytes([10; 32]),
        selector: TestCaseSelector::All,
        minimum_cases: MinimumTestCaseCount::new(1)?,
        scope: VerificationScope::Targeted,
        python_integer_translation: None,
    };
    let criterion = criterion()?.with_origin(CriterionOrigin::new(original.range(0, end)?, proof));
    let constraints = if unbound {
        vec![GoalConstraint::try_from_string(
            "preserve protected files".to_owned(),
        )?]
    } else {
        Vec::new()
    };
    let (previous, reason) = if reference.revision() == GoalContractRevision::INITIAL {
        (None, None)
    } else {
        (
            Some(GoalContractRevision::new(reference.revision().get() - 1)?),
            Some(GoalRevisionReason::try_from_string(
                "revised actual requirement".to_owned(),
            )?),
        )
    };
    Ok(GoalContract::reconstruct(
        reference.task_id(),
        reference.revision(),
        previous,
        reason,
        draft(vec![criterion], constraints)?,
        created_at,
    )?
    .with_original(original)?)
}

#[test]
fn initial_identity_needs_no_placeholder_goal_and_preserves_task() {
    let task = TaskId::from_bytes([1; 32]);
    let reference = GoalContractReference::initial(task);
    assert_eq!(reference.task_id(), task);
    assert_eq!(reference.revision(), GoalContractRevision::INITIAL);
}

#[test]
fn pending_retains_exact_whitespace_bom_crlf_and_all_original_parts() -> Result<(), Box<dyn Error>>
{
    let text = "\u{feff}  R: preserve \u{20ac}\r\n\tN: literal spaces  \r\n";
    let value = pending(1, text)?;
    assert_eq!(value.original().as_bytes(), text.as_bytes());
    assert_eq!(
        value.reference(),
        GoalContractReference::initial(value.task_id())
    );
    assert_eq!(value.created_at(), at(1)?);
    let rebuilt = value
        .original()
        .parts()
        .iter()
        .map(|range| value.original().resolve(*range))
        .collect::<Result<Vec<_>, _>>()?
        .concat();
    assert_eq!(rebuilt, text);
    Ok(())
}

#[test]
fn exact_64k_unicode_capture_restores_same_digest_without_any_draft() -> Result<(), Box<dyn Error>>
{
    let mut text = "\u{20ac}".repeat(MAX_GOAL_ORIGINAL_BYTES / 3);
    text.push('x');
    let value = pending(1, &text)?;
    assert_eq!(value.original().parts().len(), 33);
    let restored = GoalOriginalRequest::reconstruct(
        value.reference(),
        value.original().as_bytes().to_vec(),
        value.original().digest(),
    )?;
    assert_eq!(PendingGoalIntake::new(restored, value.created_at())?, value);
    Ok(())
}

#[test]
fn pending_cannot_bind_a_later_goal_revision() -> Result<(), Box<dyn Error>> {
    let value = pending(1, "R: implement behavior")?;
    let goal = proposed(
        value.reference(),
        value.original().as_str(),
        at(1)?,
        value.original().as_bytes().len(),
        false,
    )?;
    let original =
        GoalOriginalRequest::try_from_bytes(goal.next_reference()?, b"R: revision two".to_vec())?;
    assert_eq!(
        PendingGoalIntake::new(original, at(1)?),
        Err(GoalIntakeError::InitialReferenceRequired)
    );
    Ok(())
}

#[test]
fn valid_initial_promotion_retains_identity_bytes_time_and_pending() -> Result<(), Box<dyn Error>> {
    let value = pending(1, "R: implement behavior\r\n")?;
    let before = value.clone();
    let goal = proposed(
        value.reference(),
        value.original().as_str(),
        at(1)?,
        value.original().as_bytes().len(),
        false,
    )?;
    assert_eq!(value.validate_promotion(&goal), Ok(()));
    assert_eq!(goal.reference(), value.reference());
    assert_eq!(goal.original(), Some(value.original()));
    assert_eq!(
        value, before,
        "validation cannot consume or rewrite pending capture"
    );
    Ok(())
}

#[test]
fn task_revision_time_and_exact_raw_changes_are_distinct_promotion_rejections()
-> Result<(), Box<dyn Error>> {
    let text = "R: implement behavior\r\n";
    let value = pending(1, text)?;
    let other = proposed(
        GoalContractReference::initial(TaskId::from_bytes([2; 32])),
        text,
        at(1)?,
        text.len(),
        false,
    )?;
    assert_eq!(
        value.validate_promotion(&other),
        Err(GoalIntakeError::TaskMismatch)
    );
    let changed_time = proposed(value.reference(), text, at(2)?, text.len(), false)?;
    assert_eq!(
        value.validate_promotion(&changed_time),
        Err(GoalIntakeError::CreatedAtMismatch)
    );
    let changed_raw = text.replace("\r\n", "\n");
    let changed = proposed(
        value.reference(),
        &changed_raw,
        at(1)?,
        changed_raw.len(),
        false,
    )?;
    assert_eq!(
        value.validate_promotion(&changed),
        Err(GoalIntakeError::OriginalMismatch)
    );
    let valid = proposed(value.reference(), text, at(1)?, text.len(), false)?;
    let revised = proposed(valid.next_reference()?, text, at(1)?, text.len(), false)?;
    assert_eq!(
        value.validate_promotion(&revised),
        Err(GoalIntakeError::InitialReferenceRequired)
    );
    Ok(())
}

#[test]
fn legacy_and_orphan_proposals_never_replace_the_pending_original() -> Result<(), Box<dyn Error>> {
    let value = pending(1, "R: implement behavior")?;
    let legacy = GoalContract::initial(
        value.task_id(),
        draft(vec![criterion()?], Vec::new())?,
        at(1)?,
    );
    assert_eq!(
        value.validate_promotion(&legacy),
        Err(GoalIntakeError::MissingOriginal)
    );
    let origin = CriterionOrigin::new(
        value
            .original()
            .range(0, value.original().as_bytes().len())?,
        CriterionEvidenceRequirement::RuntimeNeedsRecipe,
    );
    let orphan = GoalContract::initial(
        value.task_id(),
        draft(vec![criterion()?.with_origin(origin)], Vec::new())?,
        at(1)?,
    );
    assert_eq!(
        value.validate_promotion(&orphan),
        Err(GoalIntakeError::InvalidOriginalBinding)
    );
    Ok(())
}

#[test]
fn uncovered_tail_and_only_unbound_constraint_both_keep_intake_pending()
-> Result<(), Box<dyn Error>> {
    let text = "R: implement behavior\nPreserve protected files\n";
    let value = pending(1, text)?;
    let gap = proposed(
        value.reference(),
        text,
        at(1)?,
        text.find('\n').ok_or("newline")? + 1,
        false,
    )?;
    assert!(!GoalOriginalCoverage::derive_from(&gap)?.gaps().is_empty());
    assert_eq!(
        value.validate_promotion(&gap),
        Err(GoalIntakeError::IntakeIncomplete)
    );
    let unbound = proposed(value.reference(), text, at(1)?, text.len(), true)?;
    let coverage = GoalOriginalCoverage::derive_from(&unbound)?;
    assert!(coverage.gaps().is_empty());
    assert_eq!(coverage.unbound_entries().len(), 1);
    assert_eq!(
        value.validate_promotion(&unbound),
        Err(GoalIntakeError::IntakeIncomplete)
    );
    Ok(())
}

#[test]
fn pending_debug_and_actual_promotion_error_do_not_disclose_raw_text() -> Result<(), Box<dyn Error>>
{
    let text = "PRIVATE_ORIGINAL_SENTINEL\r\n";
    let value = pending(1, text)?;
    let goal = proposed(value.reference(), text, at(2)?, text.len(), false)?;
    let failure = value
        .validate_promotion(&goal)
        .err()
        .ok_or("rejection missing")?;
    assert!(!format!("{value:?} {failure:?} {failure}").contains("PRIVATE_ORIGINAL_SENTINEL"));
    Ok(())
}
