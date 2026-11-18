//! Structural interpretation controls, never a claim of correct model extraction.
use a3_domain::*;
use std::error::Error;
type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
const RAW: &str = "Build output.\r\nDo not edit X.\r\nNo cloud.\r\nUse Rust.\r\n";
fn capture(bytes: Vec<u8>) -> Result<PendingGoalIntake> {
    Ok(PendingGoalIntake::new(
        GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(TaskId::from_bytes([1; 32])),
            bytes,
        )?,
        GoalContractTimestamp::from_unix_millis(3)?,
    )?)
}
fn pending() -> Result<PendingGoalIntake> {
    capture(RAW.as_bytes().to_vec())
}
fn range(p: &PendingGoalIntake, a: usize, b: usize) -> Result<GoalOriginalRange> {
    Ok(p.original().range(a, b)?)
}
fn full(p: &PendingGoalIntake) -> Result<GoalOriginalRange> {
    range(p, 0, p.original().as_bytes().len())
}
fn proof() -> Result<CriterionEvidenceRequirement> {
    Ok(CriterionEvidenceRequirement::ChangeOnly(
        DiffInvariantVerification::new(DiffInvariantMode::NonEmptyChanges, Vec::new())?,
    ))
}
fn finding(
    kind: GoalIntakeFindingKind,
    r: GoalOriginalRange,
    s: &str,
) -> Result<GoalIntakeFinding> {
    Ok(GoalIntakeFinding::new(kind, r, s.to_owned())?)
}
fn batch(
    r: GoalOriginalRange,
    f: Vec<GoalIntakeFinding>,
    q: Vec<GoalIntakeQuestionDraft>,
) -> Result<GoalIntakeReviewBatchProposal> {
    Ok(GoalIntakeReviewBatchProposal::new(r, f, q)?)
}
fn question(r: Vec<GoalOriginalRange>, s: &str) -> Result<GoalIntakeQuestionDraft> {
    Ok(GoalIntakeQuestionDraft::new(
        GoalIntakeQuestionKind::MissingUserChoice,
        s.to_owned(),
        r,
    )?)
}
fn goal(p: &PendingGoalIntake, constraints: Vec<GoalConstraint>) -> Result<GoalContract> {
    Ok(GoalContract::initial(
        p.task_id(),
        GoalContractDraft::new(
            GoalObjective::try_from_string("Build output".into())?,
            vec![
                AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([2; 32]),
                    AcceptanceCriterionStatement::try_from_string("Build output.".into())?,
                )
                .with_origin(CriterionOrigin::new(full(p)?, proof()?)),
            ],
            constraints,
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("Check output".into())?,
        )?,
        p.created_at(),
    )
    .with_original(p.original().clone())?)
}
#[test]
fn review_is_nonexecuting_exact_source_metadata_and_survives_restore() -> Result {
    let p = pending()?;
    let f = finding(
        GoalIntakeFindingKind::Constraint,
        range(&p, 15, 30)?,
        "Do not edit X.",
    )?;
    let review = GoalIntakeReview::begin(&p, batch(full(&p)?, vec![f], Vec::new())?)?;
    assert_eq!(review.revision().get(), 1);
    assert_eq!(review.digest(), p.original().digest());
    assert!(review.is_fully_reviewed(&p)?);
    assert_eq!(p.original().as_bytes(), RAW.as_bytes());
    assert_eq!(
        GoalIntakeReview::restore(&p, review.batches().to_vec())?,
        review
    );
    Ok(())
}
#[test]
fn missing_late_constraint_cannot_hide_behind_broad_criterion_coverage() -> Result {
    let p = pending()?;
    let r = range(&p, 15, 30)?;
    let review = GoalIntakeReview::begin(
        &p,
        batch(
            full(&p)?,
            vec![finding(
                GoalIntakeFindingKind::Constraint,
                r,
                "Do not edit X.",
            )?],
            Vec::new(),
        )?,
    )?;
    let omitted = goal(&p, Vec::new())?;
    assert!(p.validate_promotion(&omitted).is_ok());
    assert_eq!(
        review.validate_promotion(&p, &omitted),
        Err(GoalIntakeReviewError::FindingOmitted)
    );
    let kept = goal(
        &p,
        vec![GoalConstraint::try_from_string("Do not edit X.".into())?.with_origin(r)],
    )?;
    review.validate_promotion(&p, &kept)?;
    Ok(())
}
#[test]
fn complete_original_findings_remain_proposals_until_exact_goal_compilation() -> Result {
    let p = pending()?;
    let ranges = [
        range(&p, 0, 15)?,
        range(&p, 15, 31)?,
        range(&p, 31, 42)?,
        range(&p, 42, RAW.len())?,
    ];
    let outcome = GoalIntakeFindingKind::Outcome {
        requirement: AcceptanceCriterionRequirement::Must,
        evidence_requirement: proof()?,
    };
    let findings = vec![
        finding(outcome, ranges[0], "Build output.")?,
        finding(
            GoalIntakeFindingKind::Constraint,
            ranges[1],
            "Do not edit X.",
        )?,
        finding(GoalIntakeFindingKind::NonGoal, ranges[2], "No cloud.")?,
        finding(
            GoalIntakeFindingKind::ExistingUserDecision,
            ranges[3],
            "Use Rust.",
        )?,
    ];
    let review = GoalIntakeReview::begin(&p, batch(full(&p)?, findings, Vec::new())?)?;
    let g = GoalContract::initial(
        p.task_id(),
        GoalContractDraft::new(
            GoalObjective::try_from_string("Build output".into())?,
            vec![
                AcceptanceCriterion::new(
                    AcceptanceCriterionId::from_bytes([2; 32]),
                    AcceptanceCriterionStatement::try_from_string("Build output.".into())?,
                )
                .with_origin(CriterionOrigin::new(ranges[0], proof()?)),
            ],
            vec![GoalConstraint::try_from_string("Do not edit X.".into())?.with_origin(ranges[1])],
            vec![NonGoal::try_from_string("No cloud.".into())?.with_origin(ranges[2])],
            vec![UserDecision::try_from_string("Use Rust.".into())?.with_origin(ranges[3])],
            SuccessVerification::try_from_string("Check output".into())?,
        )?,
        p.created_at(),
    )
    .with_original(p.original().clone())?;
    review.validate_promotion(&p, &g)?;
    let wrong = goal(
        &p,
        vec![GoalConstraint::try_from_string("Do not edit X.".into())?.with_origin(ranges[1])],
    )?;
    assert_eq!(
        review.validate_promotion(&p, &wrong),
        Err(GoalIntakeReviewError::FindingOmitted)
    );
    Ok(())
}
#[test]
fn an_open_question_without_any_criterion_cannot_be_promoted_or_erased() -> Result {
    let p = pending()?;
    let r = range(&p, 0, 15)?;
    let mut review = GoalIntakeReview::begin(
        &p,
        batch(
            r,
            Vec::new(),
            vec![question(vec![r], "Which output do you want?")?],
        )?,
    )?;
    let old = review.clone();
    review.append(
        &p,
        batch(range(&p, 15, RAW.len())?, Vec::new(), Vec::new())?,
    )?;
    assert_eq!(review.questions().count(), 1);
    assert_eq!(review.questions().next().ok_or("question")?.id().get(), 1);
    review.validate_successor_of(&old)?;
    assert_eq!(
        review.validate_promotion(&p, &goal(&p, Vec::new())?),
        Err(GoalIntakeReviewError::OpenQuestions)
    );
    let erased = GoalIntakeReview::restore(
        &p,
        vec![
            GoalIntakeReviewBatch::reconstruct(r, Vec::new(), Vec::new())?,
            review.batches()[1].clone(),
        ],
    )?;
    assert_eq!(
        erased.validate_successor_of(&old),
        Err(GoalIntakeReviewError::InvalidHistory)
    );
    Ok(())
}
#[test]
fn unreviewed_tail_and_whitespace_remain_open() -> Result {
    let p = capture(b"x \r\n".to_vec())?;
    let mut r = GoalIntakeReview::begin(&p, batch(range(&p, 0, 1)?, Vec::new(), Vec::new())?)?;
    assert!(!r.is_fully_reviewed(&p)?);
    assert_eq!(
        r.validate_promotion(&p, &goal(&p, Vec::new())?),
        Err(GoalIntakeReviewError::IncompleteReview)
    );
    r.append(&p, batch(range(&p, 1, 4)?, Vec::new(), Vec::new())?)?;
    assert!(r.is_fully_reviewed(&p)?);
    Ok(())
}
#[test]
fn repeated_overlapping_foreign_and_oversized_windows_are_not_progress() -> Result {
    let p = capture(vec![b'a'; 4096])?;
    let mut review =
        GoalIntakeReview::begin(&p, batch(range(&p, 0, 2048)?, Vec::new(), Vec::new())?)?;
    let old = review.clone();
    assert_eq!(
        review.append(&p, batch(range(&p, 2047, 4095)?, Vec::new(), Vec::new())?),
        Err(GoalIntakeReviewError::WindowAlreadyReviewed)
    );
    assert_eq!(review, old);
    assert_eq!(
        GoalIntakeReviewBatchProposal::new(range(&p, 0, 2049)?, Vec::new(), Vec::new()),
        Err(GoalIntakeReviewError::WindowTooLarge)
    );
    let other = PendingGoalIntake::new(
        GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(TaskId::from_bytes([9; 32])),
            vec![b'a'; 4096],
        )?,
        p.created_at(),
    )?;
    assert_eq!(
        review.append(
            &p,
            batch(range(&other, 2048, 4096)?, Vec::new(), Vec::new())?
        ),
        Err(GoalIntakeReviewError::InvalidSource)
    );
    assert_eq!(review, old);
    Ok(())
}
#[test]
fn findings_must_stay_inside_the_current_window_and_duplicate_batch_values_fail() -> Result {
    let p = pending()?;
    let f = finding(
        GoalIntakeFindingKind::Constraint,
        range(&p, 15, 30)?,
        "Keep X",
    )?;
    assert_eq!(
        GoalIntakeReview::begin(&p, batch(range(&p, 0, 15)?, vec![f.clone()], Vec::new())?),
        Err(GoalIntakeReviewError::FindingOutsideWindow)
    );
    assert_eq!(
        GoalIntakeReview::begin(&p, batch(full(&p)?, vec![f.clone(), f], Vec::new())?),
        Err(GoalIntakeReviewError::DuplicateFinding)
    );
    Ok(())
}
#[test]
fn source_questions_can_link_a_prior_window_but_not_unseen_future_bytes() -> Result {
    let p = pending()?;
    let a = range(&p, 0, 15)?;
    let b = range(&p, 15, 31)?;
    let c = range(&p, 31, RAW.len())?;
    let mut review = GoalIntakeReview::begin(&p, batch(a, Vec::new(), Vec::new())?)?;
    assert_eq!(
        review.append(
            &p,
            batch(
                b,
                Vec::new(),
                vec![question(vec![b, c], "Which requirement wins?")?]
            )?
        ),
        Err(GoalIntakeReviewError::InvalidQuestionRanges)
    );
    review.append(
        &p,
        batch(
            b,
            Vec::new(),
            vec![question(vec![a, b], "Which requirement wins?")?],
        )?,
    )?;
    assert_eq!(review.questions().count(), 1);
    Ok(())
}
#[test]
fn batch_and_global_question_bounds_preserve_prior_state() -> Result {
    let p = capture(vec![b'a'; 64])?;
    let r = range(&p, 0, 1)?;
    assert_eq!(
        GoalIntakeReviewBatchProposal::new(
            r,
            vec![finding(GoalIntakeFindingKind::Constraint, r, "x")?; 5],
            Vec::new()
        ),
        Err(GoalIntakeReviewError::FindingLimit)
    );
    assert_eq!(
        GoalIntakeReviewBatchProposal::new(r, Vec::new(), vec![question(vec![r], "x")?; 2]),
        Err(GoalIntakeReviewError::QuestionLimit)
    );
    let mut review = GoalIntakeReview::begin(
        &p,
        batch(r, Vec::new(), vec![question(vec![r], "Choice 0?")?])?,
    )?;
    for i in 1..32 {
        let r = range(&p, i, i + 1)?;
        review.append(
            &p,
            batch(
                r,
                Vec::new(),
                vec![question(vec![r], &format!("Choice {i}?"))?],
            )?,
        )?;
    }
    let old = review.clone();
    let r = range(&p, 32, 33)?;
    assert_eq!(
        review.append(
            &p,
            batch(r, Vec::new(), vec![question(vec![r], "Choice 32?")?])?
        ),
        Err(GoalIntakeReviewError::QuestionLimit)
    );
    assert_eq!(review, old);
    Ok(())
}
#[test]
fn family_and_history_limits_are_hard_and_reconstruct_rejects_empty() -> Result {
    let p = capture(vec![b'a'; 256])?;
    let r = range(&p, 0, 1)?;
    let mut review = GoalIntakeReview::begin(
        &p,
        batch(
            r,
            vec![finding(GoalIntakeFindingKind::Constraint, r, "Boundary 0")?],
            Vec::new(),
        )?,
    )?;
    for i in 1..64 {
        let r = range(&p, i, i + 1)?;
        review.append(
            &p,
            batch(
                r,
                vec![finding(
                    GoalIntakeFindingKind::Constraint,
                    r,
                    &format!("Boundary {i}"),
                )?],
                Vec::new(),
            )?,
        )?;
    }
    let old = review.clone();
    let r = range(&p, 64, 65)?;
    assert_eq!(
        review.append(
            &p,
            batch(
                r,
                vec![finding(
                    GoalIntakeFindingKind::Constraint,
                    r,
                    "Boundary 64"
                )?],
                Vec::new()
            )?
        ),
        Err(GoalIntakeReviewError::FindingLimit)
    );
    assert_eq!(review, old);
    for i in 64..128 {
        review.append(&p, batch(range(&p, i, i + 1)?, Vec::new(), Vec::new())?)?;
    }
    assert_eq!(review.revision().get(), 128);
    assert_eq!(
        review.append(&p, batch(range(&p, 128, 129)?, Vec::new(), Vec::new())?),
        Err(GoalIntakeReviewError::HistoryLimit)
    );
    assert_eq!(
        GoalIntakeReview::restore(&p, Vec::new()),
        Err(GoalIntakeReviewError::HistoryLimit)
    );
    Ok(())
}
#[test]
fn aggregate_text_limit_is_atomic_and_diagnostics_never_copy_text() -> Result {
    let p = capture(vec![b'a'; 100])?;
    let mut review = None;
    for i in 0..8 {
        let r = range(&p, i, i + 1)?;
        let mut f = Vec::new();
        for (n, k) in [
            GoalIntakeFindingKind::Constraint,
            GoalIntakeFindingKind::NonGoal,
            GoalIntakeFindingKind::ExistingUserDecision,
            GoalIntakeFindingKind::Outcome {
                requirement: AcceptanceCriterionRequirement::Must,
                evidence_requirement: proof()?,
            },
        ]
        .into_iter()
        .enumerate()
        {
            f.push(finding(k, r, &format!("{i}{n}{}", "s".repeat(2046)))?);
        }
        let b = batch(r, f, Vec::new())?;
        match &mut review {
            None => review = Some(GoalIntakeReview::begin(&p, b)?),
            Some(review) => review.append(&p, b)?,
        }
    }
    let mut review = review.ok_or("review")?;
    let old = review.clone();
    let r = range(&p, 8, 9)?;
    assert_eq!(
        review.append(
            &p,
            batch(
                r,
                vec![finding(GoalIntakeFindingKind::Constraint, r, "extra")?],
                Vec::new()
            )?
        ),
        Err(GoalIntakeReviewError::TextLimit)
    );
    assert_eq!(review, old);
    let secret = "PRIVATE QUESTION TOKEN";
    let q = question(vec![r], secret)?;
    assert!(!format!("{q:?}").contains(secret));
    let f = finding(GoalIntakeFindingKind::Constraint, r, secret)?;
    assert!(!format!("{f:?}").contains(secret));
    assert!(
        !GoalIntakeReviewError::FindingOmitted
            .to_string()
            .contains(secret)
    );
    Ok(())
}
#[test]
fn exact_64k_unicode_crlf_is_considered_without_changing_any_bytes() -> Result {
    let text = format!("a{}bbb", "\u{1f4a1}".repeat(16383));
    assert_eq!(text.len(), 65536);
    let p = capture(text.as_bytes().to_vec())?;
    let mut review = None;
    for part in p.original().parts() {
        let b = batch(*part, Vec::new(), Vec::new())?;
        match &mut review {
            None => review = Some(GoalIntakeReview::begin(&p, b)?),
            Some(review) => review.append(&p, b)?,
        }
    }
    let review = review.ok_or("review")?;
    assert_eq!(review.revision().get(), 33);
    assert!(review.is_fully_reviewed(&p)?);
    assert_eq!(p.original().as_bytes(), text.as_bytes());
    assert_eq!(
        GoalIntakeReview::restore(&p, review.batches().to_vec())?,
        review
    );
    Ok(())
}
#[test]
fn restored_question_ordinals_cannot_be_forged() -> Result {
    let p = pending()?;
    let q = question(vec![full(&p)?], "Choose?")?;
    let bad = GoalIntakeReviewBatch::reconstruct(
        full(&p)?,
        Vec::new(),
        vec![GoalIntakeOpenQuestion::reconstruct(
            GoalIntakeQuestionId::new(2)?,
            q,
        )],
    )?;
    assert_eq!(
        GoalIntakeReview::restore(&p, vec![bad]),
        Err(GoalIntakeReviewError::InvalidHistory)
    );
    Ok(())
}

fn path_obligation(bytes: usize) -> Result<CriterionEvidenceRequirement> {
    Ok(CriterionEvidenceRequirement::ChangeOnly(
        DiffInvariantVerification::new(
            DiffInvariantMode::ExactPaths,
            vec![RepositoryPath::try_from_bytes(vec![b'a'; bytes])?],
        )?,
    ))
}
#[test]
fn requirement_paths_are_bounded_across_history_without_truncation() -> Result {
    let p = capture(vec![b'a'; 4])?;
    let r = range(&p, 0, 1)?;
    let kind = |bytes| -> Result<GoalIntakeFindingKind> {
        Ok(GoalIntakeFindingKind::Outcome {
            requirement: AcceptanceCriterionRequirement::Must,
            evidence_requirement: path_obligation(bytes)?,
        })
    };
    assert_eq!(
        GoalIntakeFinding::new(kind(65537)?, r, "result".into()),
        Err(GoalIntakeReviewError::RequirementPathLimit)
    );
    let mut review = GoalIntakeReview::begin(
        &p,
        batch(r, vec![finding(kind(32768)?, r, "first")?], Vec::new())?,
    )?;
    let r = range(&p, 1, 2)?;
    review.append(
        &p,
        batch(r, vec![finding(kind(32768)?, r, "second")?], Vec::new())?,
    )?;
    let old = review.clone();
    let r = range(&p, 2, 3)?;
    assert_eq!(
        review.append(
            &p,
            batch(r, vec![finding(kind(1)?, r, "third")?], Vec::new())?
        ),
        Err(GoalIntakeReviewError::RequirementPathLimit)
    );
    assert_eq!(review, old);
    assert_eq!(
        GoalIntakeReview::restore(&p, review.batches().to_vec())?,
        review
    );
    Ok(())
}

#[test]
fn outcome_debug_does_not_dump_path_or_selector_payload() -> Result {
    let p = pending()?;
    let secret = "PRIVATE_ORIGINAL_PATH";
    let evidence = CriterionEvidenceRequirement::ChangeOnly(DiffInvariantVerification::new(
        DiffInvariantMode::ExactPaths,
        vec![RepositoryPath::try_from_bytes(secret.as_bytes().to_vec())?],
    )?);
    let kind = GoalIntakeFindingKind::Outcome {
        requirement: AcceptanceCriterionRequirement::Must,
        evidence_requirement: evidence,
    };
    assert!(!format!("{kind:?}").contains(secret));
    let f = finding(kind, full(&p)?, "proposed outcome")?;
    assert!(!format!("{f:?}").contains(secret));
    Ok(())
}
