use super::{PrepareGoalFromIntakeReviewFailure as Failure, prepare_goal_from_intake_review};
use a3_domain::{
    AcceptanceCriterionRequirement, CriterionEvidenceRequirement, DiffInvariantMode,
    DiffInvariantVerification, GoalContractReference, GoalContractTimestamp, GoalIntakeError,
    GoalIntakeFinding, GoalIntakeFindingKind, GoalIntakeQuestionDraft, GoalIntakeQuestionKind,
    GoalIntakeReview, GoalIntakeReviewBatchProposal, GoalIntakeReviewError, GoalOriginalRange,
    GoalOriginalRequest, PendingGoalIntake, RepositoryPath, TaskId,
};
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn pending(text: &str, id: u8) -> TestResult<PendingGoalIntake> {
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(TaskId::from_bytes([id; 32])),
        text.as_bytes().to_vec(),
    )?;
    Ok(PendingGoalIntake::new(
        original,
        GoalContractTimestamp::from_unix_millis(12)?,
    )?)
}

fn outcome(requirement: AcceptanceCriterionRequirement) -> GoalIntakeFindingKind {
    GoalIntakeFindingKind::Outcome {
        requirement,
        evidence_requirement: CriterionEvidenceRequirement::RuntimeNeedsRecipe,
    }
}

fn literal(
    pending: &PendingGoalIntake,
    kind: GoalIntakeFindingKind,
    range: GoalOriginalRange,
) -> TestResult<GoalIntakeFinding> {
    Ok(GoalIntakeFinding::new(
        kind,
        range,
        pending.original().resolve(range)?.to_owned(),
    )?)
}

#[test]
fn compiles_exact_multi_window_goal_in_original_order_with_typed_proof() -> TestResult {
    let should_text = "Prefer gr\u{fc}ne examples.\r\n";
    let must_text = "Update the docs.\r\nRetain their encoding.\r\n";
    let constraint_text = "Keep source files.\r\n";
    let non_goal_text = "Exclude screenshots.\r\n";
    let decision_text = "Use the existing README.";
    let text = format!("{should_text}{must_text}{constraint_text}{non_goal_text}{decision_text}");
    let pending = pending(&text, 31)?;
    let original = pending.original();
    let should_end = should_text.len();
    let must_end = should_end + must_text.len();
    let constraint_end = must_end + constraint_text.len();
    let non_goal_end = constraint_end + non_goal_text.len();
    let should_range = original.range(0, should_end)?;
    let must_range = original.range(should_end, must_end)?;
    let constraint_range = original.range(must_end, constraint_end)?;
    let non_goal_range = original.range(constraint_end, non_goal_end)?;
    let decision_range = original.range(non_goal_end, text.len())?;
    let proof = CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
        DiffInvariantMode::ExactPaths,
        vec![RepositoryPath::try_from_bytes(b"README.md".to_vec())?],
    )?);

    let earlier = GoalIntakeReviewBatchProposal::new(
        original.range(0, must_end)?,
        vec![
            literal(
                &pending,
                GoalIntakeFindingKind::Outcome {
                    requirement: AcceptanceCriterionRequirement::Must,
                    evidence_requirement: proof.clone(),
                },
                must_range,
            )?,
            literal(
                &pending,
                outcome(AcceptanceCriterionRequirement::Should),
                should_range,
            )?,
        ],
        Vec::new(),
    )?;
    let later = GoalIntakeReviewBatchProposal::new(
        original.range(must_end, text.len())?,
        vec![
            literal(
                &pending,
                GoalIntakeFindingKind::Constraint,
                constraint_range,
            )?,
            literal(&pending, GoalIntakeFindingKind::NonGoal, non_goal_range)?,
            literal(
                &pending,
                GoalIntakeFindingKind::ExistingUserDecision,
                decision_range,
            )?,
        ],
        Vec::new(),
    )?;
    let mut review = GoalIntakeReview::begin(&pending, later.clone())?;
    review.append(&pending, earlier.clone())?;
    let goal = prepare_goal_from_intake_review(&pending, &review)?;
    assert_eq!(goal.original(), Some(original));
    assert_eq!(goal.reference(), pending.reference());
    assert_eq!(goal.created_at(), pending.created_at());
    assert_eq!(
        goal.draft().objective().as_str(),
        "Update the docs.\nRetain their encoding."
    );
    let [should, must] = goal.draft().acceptance_criteria() else {
        return Err("wrong criteria".into());
    };
    assert_ne!(should.id(), must.id());
    assert_eq!(should.requirement(), AcceptanceCriterionRequirement::Should);
    assert_eq!(
        should.origin().ok_or("should origin")?.original_range(),
        should_range
    );
    assert_eq!(must.requirement(), AcceptanceCriterionRequirement::Must);
    assert_eq!(
        must.origin().ok_or("must origin")?.original_range(),
        must_range
    );
    assert_eq!(
        must.origin().ok_or("must origin")?.evidence_requirement(),
        &proof
    );
    assert_eq!(
        goal.draft().constraints()[0].origin(),
        Some(constraint_range)
    );
    assert_eq!(goal.draft().non_goals()[0].origin(), Some(non_goal_range));
    assert_eq!(
        goal.draft().user_decisions()[0].origin(),
        Some(decision_range)
    );
    review.validate_promotion(&pending, &goal)?;

    let mut forward = GoalIntakeReview::begin(&pending, earlier)?;
    forward.append(&pending, later)?;
    assert_eq!(prepare_goal_from_intake_review(&pending, &forward)?, goal);
    Ok(())
}

#[test]
fn incomplete_review_and_open_questions_halt_preparation() -> TestResult {
    let text = "Update docs.\r\nWhich language?";
    let pending = pending(text, 31)?;
    let original = pending.original();
    let outcome_range = original.range(0, "Update docs.\r\n".len())?;
    let question_range = original.range(outcome_range.end_byte() as usize, text.len())?;
    let mut review = GoalIntakeReview::begin(
        &pending,
        GoalIntakeReviewBatchProposal::new(
            outcome_range,
            vec![literal(
                &pending,
                outcome(AcceptanceCriterionRequirement::Must),
                outcome_range,
            )?],
            Vec::new(),
        )?,
    )?;
    assert_eq!(
        prepare_goal_from_intake_review(&pending, &review),
        Err(Failure::Review(GoalIntakeReviewError::IncompleteReview))
    );
    review.append(
        &pending,
        GoalIntakeReviewBatchProposal::new(
            question_range,
            Vec::new(),
            vec![GoalIntakeQuestionDraft::new(
                GoalIntakeQuestionKind::MissingUserChoice,
                "Which language should the docs use?".to_owned(),
                vec![question_range],
            )?],
        )?,
    )?;
    assert_eq!(
        prepare_goal_from_intake_review(&pending, &review),
        Err(Failure::Review(GoalIntakeReviewError::OpenQuestions))
    );
    Ok(())
}

#[test]
fn reviewed_but_unassigned_whitespace_blocks_promotion() -> TestResult {
    let text = "Update docs.";
    let pending = pending(&format!("{text}\r\n "), 31)?;
    let original = pending.original();
    let statement = original.range(0, text.len())?;
    let whole = original.range(0, original.as_bytes().len())?;
    let review = GoalIntakeReview::begin(
        &pending,
        GoalIntakeReviewBatchProposal::new(
            whole,
            vec![literal(
                &pending,
                outcome(AcceptanceCriterionRequirement::Must),
                statement,
            )?],
            Vec::new(),
        )?,
    )?;
    assert!(review.is_fully_reviewed(&pending)?);
    assert_eq!(
        prepare_goal_from_intake_review(&pending, &review),
        Err(Failure::Promotion(GoalIntakeError::IntakeIncomplete))
    );
    Ok(())
}

#[test]
fn missing_outcomes_and_non_literal_findings_halt_preparation() -> TestResult {
    let text = "Keep source files.";
    let pending = pending(text, 31)?;
    let whole = pending.original().range(0, text.len())?;
    let no_outcome = GoalIntakeReview::begin(
        &pending,
        GoalIntakeReviewBatchProposal::new(
            whole,
            vec![literal(&pending, GoalIntakeFindingKind::Constraint, whole)?],
            Vec::new(),
        )?,
    )?;
    assert_eq!(
        prepare_goal_from_intake_review(&pending, &no_outcome),
        Err(Failure::MissingOutcome)
    );
    let paraphrase = GoalIntakeReview::begin(
        &pending,
        GoalIntakeReviewBatchProposal::new(
            whole,
            vec![GoalIntakeFinding::new(
                outcome(AcceptanceCriterionRequirement::Must),
                whole,
                "Delete source files.".to_owned(),
            )?],
            Vec::new(),
        )?,
    )?;
    assert_eq!(
        prepare_goal_from_intake_review(&pending, &paraphrase),
        Err(Failure::NonLiteralFinding)
    );
    Ok(())
}

#[test]
fn should_only_goal_uses_first_should_and_criterion_id_is_goal_bound() -> TestResult {
    let text = "Prefer localized documentation.";
    let first = pending(text, 31)?;
    let second = pending(text, 32)?;
    let review = |capture: &PendingGoalIntake| -> TestResult<GoalIntakeReview> {
        let whole = capture.original().range(0, text.len())?;
        Ok(GoalIntakeReview::begin(
            capture,
            GoalIntakeReviewBatchProposal::new(
                whole,
                vec![literal(
                    capture,
                    outcome(AcceptanceCriterionRequirement::Should),
                    whole,
                )?],
                Vec::new(),
            )?,
        )?)
    };
    let first_review = review(&first)?;
    assert_eq!(
        prepare_goal_from_intake_review(&second, &first_review),
        Err(Failure::Review(GoalIntakeReviewError::CaptureMismatch))
    );
    let first_goal = prepare_goal_from_intake_review(&first, &first_review)?;
    let second_goal = prepare_goal_from_intake_review(&second, &review(&second)?)?;
    assert_eq!(first_goal.draft().objective().as_str(), text);
    assert_ne!(
        first_goal.draft().acceptance_criteria()[0].id(),
        second_goal.draft().acceptance_criteria()[0].id(),
    );
    Ok(())
}
