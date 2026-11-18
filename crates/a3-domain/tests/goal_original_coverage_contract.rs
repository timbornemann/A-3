//! Structural byte coverage does not erase source glyphs or claim semantic verification.

use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
    CriterionEvidenceRequirement, CriterionOrigin, GoalConstraint, GoalContract, GoalContractDraft,
    GoalContractTimestamp, GoalObjective, GoalOriginalBindingError, GoalOriginalCoverage,
    GoalOriginalCoverageError, GoalOriginalEntryKind, GoalOriginalRequest, MAX_GOAL_ORIGINAL_BYTES,
    NonGoal, SuccessVerification, TaskId, UserDecision,
};
use std::error::Error;

fn criterion(index: u8) -> Result<AcceptanceCriterion, Box<dyn Error>> {
    Ok(AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([index; 32]),
        AcceptanceCriterionStatement::try_from_string(format!("required outcome {index}"))?,
    ))
}

fn draft(
    criteria: Vec<AcceptanceCriterion>,
    counts: [usize; 3],
) -> Result<GoalContractDraft, Box<dyn Error>> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("the requested result".to_owned())?,
        criteria,
        (0..counts[0])
            .map(|index| GoalConstraint::try_from_string(format!("constraint {index}")))
            .collect::<Result<Vec<_>, _>>()?,
        (0..counts[1])
            .map(|index| NonGoal::try_from_string(format!("excluded outcome {index}")))
            .collect::<Result<Vec<_>, _>>()?,
        (0..counts[2])
            .map(|index| UserDecision::try_from_string(format!("confirmed choice {index}")))
            .collect::<Result<Vec<_>, _>>()?,
        SuccessVerification::try_from_string("verify the requested behavior".to_owned())?,
    )?)
}

fn legacy() -> Result<GoalContract, Box<dyn Error>> {
    Ok(GoalContract::initial(
        TaskId::from_bytes([1; 32]),
        draft(vec![criterion(1)?], [0; 3])?,
        GoalContractTimestamp::from_unix_millis(1)?,
    ))
}

fn sourced(
    text: &str,
    ranges: &[(usize, usize)],
    counts: [usize; 3],
) -> Result<GoalContract, Box<dyn Error>> {
    let base = legacy()?;
    let original = GoalOriginalRequest::try_from_bytes(base.reference(), text.as_bytes().to_vec())?;
    let mut criteria = Vec::new();
    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        criteria.push(
            criterion(u8::try_from(index + 1)?)?.with_origin(CriterionOrigin::new(
                original.range(start, end)?,
                CriterionEvidenceRequirement::RuntimeNeedsRecipe,
            )),
        );
    }
    Ok(
        GoalContract::initial(base.task_id(), draft(criteria, counts)?, base.created_at())
            .with_original(original)?,
    )
}

fn gap_texts<'a>(
    goal: &'a GoalContract,
    coverage: &GoalOriginalCoverage,
) -> Result<Vec<&'a str>, Box<dyn Error>> {
    let original = goal.original().ok_or("missing test original")?;
    Ok(coverage
        .gaps()
        .iter()
        .copied()
        .map(|gap| original.resolve(gap))
        .collect::<Result<Vec<_>, _>>()?)
}

#[test]
fn one_small_fully_bound_requirement_is_structurally_complete_without_text_rules()
-> Result<(), Box<dyn Error>> {
    let goal = sourced("R", &[(0, 1)], [0; 3])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.goal(), goal.reference());
    assert_eq!(
        coverage.digest(),
        goal.original().ok_or("missing test original")?.digest()
    );
    assert_eq!(coverage.covered_bytes(), 1);
    assert!(coverage.gaps().is_empty());
    assert!(coverage.unbound_entries().is_empty());
    assert!(coverage.is_complete());
    // RuntimeNeedsRecipe deliberately remains unavailable: coverage is not a Done receipt.
    assert!(matches!(
        goal.draft().acceptance_criteria()[0]
            .origin()
            .ok_or("missing test origin")?
            .evidence_requirement(),
        CriterionEvidenceRequirement::RuntimeNeedsRecipe
    ));
    Ok(())
}

#[test]
fn lost_tail_is_reported_with_the_exact_original_digest_and_bytes() -> Result<(), Box<dyn Error>> {
    let text = "R01: do it\r\nR02: late requirement";
    let tail = text.find("R02").ok_or("missing fixture tail")?;
    let goal = sourced(text, &[(0, tail)], [0; 3])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.covered_bytes() as usize, tail);
    assert_eq!(gap_texts(&goal, &coverage)?, vec!["R02: late requirement"]);
    assert_eq!(coverage.gaps()[0].start_byte() as usize, tail);
    assert_eq!(coverage.gaps()[0].end_byte() as usize, text.len());
    assert!(!coverage.is_complete());
    Ok(())
}

#[test]
fn whitespace_only_gaps_are_not_erased_as_formatting() -> Result<(), Box<dyn Error>> {
    for whitespace in [" ", "\r\n", "\t", " \r\n\t "] {
        let text = format!("left{whitespace}right");
        let goal = sourced(&text, &[(0, 4), (4 + whitespace.len(), text.len())], [0; 3])?;
        let coverage = GoalOriginalCoverage::derive_from(&goal)?;
        assert_eq!(coverage.covered_bytes(), 9);
        assert_eq!(gap_texts(&goal, &coverage)?, vec![whitespace]);
        assert!(!coverage.is_complete());
    }
    Ok(())
}

#[test]
fn missing_unicode_glyphs_are_separate_ordered_utf8_safe_ranges() -> Result<(), Box<dyn Error>> {
    let text = "a€b🚀c";
    let goal = sourced(text, &[(9, 10), (0, 1), (4, 5)], [0; 3])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.covered_bytes(), 3);
    assert_eq!(gap_texts(&goal, &coverage)?, vec!["€", "🚀"]);
    assert_eq!(
        coverage
            .gaps()
            .iter()
            .map(|gap| (gap.start_byte(), gap.end_byte()))
            .collect::<Vec<_>>(),
        vec![(1, 4), (5, 9)]
    );
    assert!(!coverage.is_complete());
    Ok(())
}

#[test]
fn overlaps_and_nested_ranges_count_only_their_union() -> Result<(), Box<dyn Error>> {
    let goal = sourced("abcdefghij", &[(4, 8), (2, 4), (0, 6), (1, 5)], [0; 3])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.covered_bytes(), 8);
    assert_eq!(gap_texts(&goal, &coverage)?, vec!["ij"]);
    assert!(!coverage.is_complete());
    Ok(())
}

#[test]
fn adjacent_out_of_order_ranges_leave_no_artificial_empty_gap() -> Result<(), Box<dyn Error>> {
    let goal = sourced("abcdef", &[(4, 6), (0, 2), (2, 4)], [0; 3])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.covered_bytes(), 6);
    assert!(coverage.gaps().is_empty());
    assert!(coverage.is_complete());
    Ok(())
}

#[test]
fn noncriteria_entries_remain_unbound_even_if_a_criterion_covers_every_byte()
-> Result<(), Box<dyn Error>> {
    let goal = sourced("all original bytes", &[(0, 18)], [2, 1, 2])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert!(coverage.gaps().is_empty());
    assert_eq!(coverage.covered_bytes(), 18);
    assert_eq!(
        coverage
            .unbound_entries()
            .iter()
            .map(|entry| (entry.kind(), entry.ordinal().get()))
            .collect::<Vec<_>>(),
        vec![
            (GoalOriginalEntryKind::Constraint, 1),
            (GoalOriginalEntryKind::Constraint, 2),
            (GoalOriginalEntryKind::NonGoal, 1),
            (GoalOriginalEntryKind::UserDecision, 1),
            (GoalOriginalEntryKind::UserDecision, 2)
        ]
    );
    assert!(!coverage.is_complete());
    Ok(())
}

#[test]
fn all_three_unbound_collections_keep_every_ordinal_through_64() -> Result<(), Box<dyn Error>> {
    let goal = sourced("R", &[(0, 1)], [64; 3])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.unbound_entries().len(), 192);
    for (index, kind) in [
        GoalOriginalEntryKind::Constraint,
        GoalOriginalEntryKind::NonGoal,
        GoalOriginalEntryKind::UserDecision,
    ]
    .into_iter()
    .enumerate()
    {
        for (offset, entry) in coverage.unbound_entries()[index * 64..(index + 1) * 64]
            .iter()
            .enumerate()
        {
            assert_eq!(entry.kind(), kind);
            assert_eq!(entry.ordinal().get() as usize, offset + 1);
        }
    }
    assert!(!coverage.is_complete());
    Ok(())
}

#[test]
fn legacy_and_invalid_orphan_bindings_return_distinct_typed_errors() -> Result<(), Box<dyn Error>> {
    let legacy = legacy()?;
    assert_eq!(
        GoalOriginalCoverage::derive_from(&legacy),
        Err(GoalOriginalCoverageError::MissingOriginal)
    );
    let source = sourced("R", &[(0, 1)], [0; 3])?;
    let orphan = GoalContract::initial(
        source.task_id(),
        source.draft().clone(),
        source.created_at(),
    );
    assert_eq!(
        GoalOriginalCoverage::derive_from(&orphan),
        Err(GoalOriginalCoverageError::InvalidBinding(
            GoalOriginalBindingError::MissingOriginal
        ))
    );
    Ok(())
}

#[test]
fn maximum_criterion_count_can_produce_all_65_gaps_without_truncation() -> Result<(), Box<dyn Error>>
{
    let text = " x".repeat(64) + " ";
    let ranges = (0..64)
        .map(|index| (index * 2 + 1, index * 2 + 2))
        .collect::<Vec<_>>();
    let goal = sourced(&text, &ranges, [0; 3])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.covered_bytes(), 64);
    assert_eq!(coverage.gaps().len(), 65);
    assert!(gap_texts(&goal, &coverage)?.iter().all(|gap| *gap == " "));
    assert!(!coverage.is_complete());
    Ok(())
}

#[test]
fn exact_64k_coverage_is_deterministic_and_keeps_the_original_unchanged()
-> Result<(), Box<dyn Error>> {
    let text = "a".repeat(MAX_GOAL_ORIGINAL_BYTES);
    let goal = sourced(&text, &[(0, text.len())], [0; 3])?;
    let first = GoalOriginalCoverage::derive_from(&goal)?;
    let second = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(first, second);
    assert_eq!(first.covered_bytes() as usize, MAX_GOAL_ORIGINAL_BYTES);
    assert!(first.is_complete());
    assert_eq!(
        goal.original().ok_or("missing test original")?.as_bytes(),
        text.as_bytes()
    );
    Ok(())
}

#[test]
fn report_debug_and_errors_do_not_copy_original_or_constraint_text() -> Result<(), Box<dyn Error>> {
    let marker = "private-original-unmapped-marker";
    let goal = sourced(marker, &[(0, 1)], [1; 3])?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    let debug = format!("{coverage:?}");
    assert!(!debug.contains(marker));
    assert!(!debug.contains("constraint 0"));
    for error in [
        GoalOriginalCoverageError::MissingOriginal,
        GoalOriginalCoverageError::InvalidBinding(GoalOriginalBindingError::MissingOriginal),
        GoalOriginalCoverageError::ByteCountOverflow,
        GoalOriginalCoverageError::UnboundEntryLimitExceeded,
    ] {
        assert!(!format!("{error} {error:?}").contains(marker));
    }
    Ok(())
}
