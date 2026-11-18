//! The three existing goal text values retain their own exact optional original ranges.

use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement,
    CriterionEvidenceRequirement, CriterionOrigin, DiscoveredCommandId, GoalConstraint,
    GoalContract, GoalContractCollection, GoalContractDraft, GoalContractDraftError,
    GoalContractHistory, GoalContractHistoryError, GoalContractReference,
    GoalContractRevisionFailure, GoalContractTimestamp, GoalObjective, GoalOriginalBindingError,
    GoalOriginalCoverage, GoalOriginalEntryKind, GoalOriginalError, GoalOriginalRequest,
    GoalRevisionReason, MinimumTestCaseCount, NonGoal, SuccessVerification, TaskId,
    TestCaseSelector, UserDecision, VerificationScope,
};
use std::collections::{BTreeSet, HashSet};
use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Default)]
struct Entries {
    constraints: Vec<GoalConstraint>,
    non_goals: Vec<NonGoal>,
    decisions: Vec<UserDecision>,
}

fn proof(minimum: u32) -> Result<CriterionEvidenceRequirement, Box<dyn Error>> {
    Ok(CriterionEvidenceRequirement::ExistingTest {
        command: DiscoveredCommandId::from_bytes([9; 32]),
        selector: TestCaseSelector::All,
        minimum_cases: MinimumTestCaseCount::new(minimum)?,
        scope: VerificationScope::Package,
        python_integer_translation: None,
    })
}

fn draft(
    criteria: Vec<AcceptanceCriterion>,
    entries: Entries,
) -> Result<GoalContractDraft, Box<dyn Error>> {
    Ok(GoalContractDraft::new(
        GoalObjective::try_from_string("request".to_owned())?,
        criteria,
        entries.constraints,
        entries.non_goals,
        entries.decisions,
        SuccessVerification::try_from_string("verify requested behavior".to_owned())?,
    )?)
}

fn criterion() -> Result<AcceptanceCriterion, Box<dyn Error>> {
    Ok(AcceptanceCriterion::new(
        AcceptanceCriterionId::from_bytes([1; 32]),
        AcceptanceCriterionStatement::try_from_string("criterion".to_owned())?,
    ))
}

fn legacy(task: u8) -> Result<GoalContract, Box<dyn Error>> {
    Ok(GoalContract::initial(
        TaskId::from_bytes([task; 32]),
        draft(vec![criterion()?], Entries::default())?,
        GoalContractTimestamp::from_unix_millis(1)?,
    ))
}

fn source(
    reference: GoalContractReference,
    text: &str,
) -> Result<GoalOriginalRequest, Box<dyn Error>> {
    Ok(GoalOriginalRequest::try_from_bytes(
        reference,
        text.as_bytes().to_vec(),
    )?)
}

fn grounded_draft(
    original: &GoalOriginalRequest,
    span: (usize, usize),
    entries: Entries,
    requirement: CriterionEvidenceRequirement,
) -> Result<GoalContractDraft, Box<dyn Error>> {
    draft(
        vec![criterion()?.with_origin(CriterionOrigin::new(
            original.range(span.0, span.1)?,
            requirement,
        ))],
        entries,
    )
}

fn entries_for(original: &GoalOriginalRequest) -> Result<Entries, Box<dyn Error>> {
    Ok(Entries {
        constraints: vec![
            GoalConstraint::try_from_string("guard".to_owned())?.with_origin(original.range(1, 2)?),
        ],
        non_goals: vec![
            NonGoal::try_from_string("omit".to_owned())?.with_origin(original.range(2, 3)?),
        ],
        decisions: vec![
            UserDecision::try_from_string("confirmed".to_owned())?
                .with_origin(original.range(3, 4)?),
        ],
    })
}

fn full_goal(text: &str) -> Result<GoalContract, Box<dyn Error>> {
    let base = legacy(1)?;
    let original = source(base.reference(), text)?;
    Ok(GoalContract::initial(
        base.task_id(),
        grounded_draft(&original, (0, 1), entries_for(&original)?, proof(1)?)?,
        base.created_at(),
    )
    .with_original(original)?)
}

fn hash(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn legacy_normalization_and_text_access_remain_unchanged_for_all_three_types()
-> Result<(), Box<dyn Error>> {
    let original = source(legacy(1)?.reference(), "RCNU")?;
    let range = original.range(0, 1)?;
    macro_rules! check {
        ($value:ty) => {{
            let legacy = <$value>::try_from_string("  same\r\ntext  ".to_owned())?;
            assert_eq!(legacy.as_str(), "same\ntext");
            assert!(legacy.origin().is_none());
            let annotated = legacy.clone().with_origin(range);
            assert_eq!(annotated.as_str(), legacy.as_str());
            assert_eq!(annotated.origin(), Some(range));
            assert_eq!(annotated.clone(), annotated);
            assert!(legacy.origin().is_none());
        }};
    }
    check!(GoalConstraint);
    check!(NonGoal);
    check!(UserDecision);
    Ok(())
}

#[test]
fn equality_hash_and_order_retain_origin_presence_and_coordinates() -> Result<(), Box<dyn Error>> {
    let original = source(legacy(1)?.reference(), "RCNU")?;
    macro_rules! check {
        ($value:ty) => {{
            let plain = <$value>::try_from_string("same".to_owned())?;
            let a = plain.clone().with_origin(original.range(0, 1)?);
            let b = plain.clone().with_origin(original.range(1, 2)?);
            assert_ne!(plain, a);
            assert_ne!(a, b);
            assert_ne!(hash(&plain), hash(&a));
            assert_ne!(hash(&a), hash(&b));
            assert_eq!(
                BTreeSet::from([plain.clone(), a.clone(), b.clone()]).len(),
                3
            );
            assert_eq!(HashSet::from([plain, a, b]).len(), 3);
        }};
    }
    check!(GoalConstraint);
    check!(NonGoal);
    check!(UserDecision);
    Ok(())
}

#[test]
fn content_dedup_rejects_equal_normalized_text_despite_different_or_missing_origins()
-> Result<(), Box<dyn Error>> {
    let original = source(legacy(1)?.reference(), "RCNU")?;
    for second_origin in [None, Some(original.range(1, 2)?)] {
        let mut c = GoalConstraint::try_from_string("same\ntext".to_owned())?;
        let mut n = NonGoal::try_from_string("same\ntext".to_owned())?;
        let mut u = UserDecision::try_from_string("same\ntext".to_owned())?;
        if let Some(range) = second_origin {
            c = c.with_origin(range);
            n = n.with_origin(range);
            u = u.with_origin(range);
        }
        let cases = [
            (
                Entries {
                    constraints: vec![
                        GoalConstraint::try_from_string(" same\r\ntext ".to_owned())?
                            .with_origin(original.range(0, 1)?),
                        c,
                    ],
                    ..Entries::default()
                },
                GoalContractCollection::Constraints,
            ),
            (
                Entries {
                    non_goals: vec![
                        NonGoal::try_from_string(" same\r\ntext ".to_owned())?
                            .with_origin(original.range(0, 1)?),
                        n,
                    ],
                    ..Entries::default()
                },
                GoalContractCollection::NonGoals,
            ),
            (
                Entries {
                    decisions: vec![
                        UserDecision::try_from_string(" same\r\ntext ".to_owned())?
                            .with_origin(original.range(0, 1)?),
                        u,
                    ],
                    ..Entries::default()
                },
                GoalContractCollection::UserDecisions,
            ),
        ];
        for (entries, collection) in cases {
            let result = GoalContractDraft::new(
                GoalObjective::try_from_string("request".to_owned())?,
                vec![criterion()?],
                entries.constraints,
                entries.non_goals,
                entries.decisions,
                SuccessVerification::try_from_string("verify".to_owned())?,
            );
            assert_eq!(
                result,
                Err(GoalContractDraftError::DuplicateItem(collection))
            );
        }
    }
    Ok(())
}

#[test]
fn each_noncriterion_origin_requires_an_original_even_when_criteria_are_legacy()
-> Result<(), Box<dyn Error>> {
    let base = legacy(1)?;
    let original = source(base.reference(), "RCNU")?;
    let cases = [
        Entries {
            constraints: vec![
                GoalConstraint::try_from_string("guard".to_owned())?
                    .with_origin(original.range(1, 2)?),
            ],
            ..Entries::default()
        },
        Entries {
            non_goals: vec![
                NonGoal::try_from_string("omit".to_owned())?.with_origin(original.range(2, 3)?),
            ],
            ..Entries::default()
        },
        Entries {
            decisions: vec![
                UserDecision::try_from_string("confirmed".to_owned())?
                    .with_origin(original.range(3, 4)?),
            ],
            ..Entries::default()
        },
    ];
    for entries in cases {
        let orphan = GoalContract::initial(
            base.task_id(),
            draft(vec![criterion()?], entries)?,
            base.created_at(),
        );
        assert_eq!(
            orphan.validate_original_binding(),
            Err(GoalOriginalBindingError::MissingOriginal)
        );
    }
    Ok(())
}

#[test]
fn foreign_task_revision_and_stale_digest_are_rejected_for_each_entry_kind()
-> Result<(), Box<dyn Error>> {
    let base = legacy(1)?;
    let original = source(base.reference(), "RCNU")?;
    for (foreign, error) in [
        (
            source(legacy(2)?.reference(), "RCNU")?,
            GoalOriginalError::GoalMismatch,
        ),
        (
            source(base.next_reference()?, "RCNU")?,
            GoalOriginalError::GoalMismatch,
        ),
        (
            source(base.reference(), "older")?,
            GoalOriginalError::DigestMismatch,
        ),
    ] {
        for kind in [
            GoalOriginalEntryKind::Constraint,
            GoalOriginalEntryKind::NonGoal,
            GoalOriginalEntryKind::UserDecision,
        ] {
            let range = foreign.range(0, 1)?;
            let entries = match kind {
                GoalOriginalEntryKind::Constraint => Entries {
                    constraints: vec![
                        GoalConstraint::try_from_string("guard".to_owned())?.with_origin(range),
                    ],
                    ..Entries::default()
                },
                GoalOriginalEntryKind::NonGoal => Entries {
                    non_goals: vec![
                        NonGoal::try_from_string("omit".to_owned())?.with_origin(range),
                    ],
                    ..Entries::default()
                },
                GoalOriginalEntryKind::UserDecision => Entries {
                    decisions: vec![
                        UserDecision::try_from_string("confirmed".to_owned())?.with_origin(range),
                    ],
                    ..Entries::default()
                },
            };
            let result = GoalContract::initial(
                base.task_id(),
                grounded_draft(&original, (0, 1), entries, proof(1)?)?,
                base.created_at(),
            )
            .with_original(original.clone());
            match result {
                Err(GoalOriginalBindingError::InvalidEntryOrigin {
                    entry,
                    error: actual,
                }) => {
                    assert_eq!(entry.kind(), kind);
                    assert_eq!(entry.ordinal().get(), 1);
                    assert_eq!(actual, error);
                }
                other => {
                    return Err(format!("expected entry binding rejection, got {other:?}").into());
                }
            }
        }
    }
    Ok(())
}

#[test]
fn all_four_origin_families_union_exact_unicode_and_crlf_bytes() -> Result<(), Box<dyn Error>> {
    let base = legacy(1)?;
    let text = "R€\r\n🚀U";
    let original = source(base.reference(), text)?;
    let entries = Entries {
        constraints: vec![
            GoalConstraint::try_from_string("guard".to_owned())?.with_origin(original.range(1, 6)?),
        ],
        non_goals: vec![
            NonGoal::try_from_string("omit".to_owned())?.with_origin(original.range(6, 10)?),
        ],
        decisions: vec![
            UserDecision::try_from_string("confirmed".to_owned())?
                .with_origin(original.range(10, 11)?),
        ],
    };
    let goal = GoalContract::initial(
        base.task_id(),
        grounded_draft(&original, (0, 1), entries, proof(1)?)?,
        base.created_at(),
    )
    .with_original(original)?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.covered_bytes() as usize, text.len());
    assert!(coverage.is_complete());
    assert_eq!(
        goal.original().ok_or("missing test original")?.as_bytes(),
        text.as_bytes()
    );
    Ok(())
}

#[test]
fn only_missing_annotations_remain_unbound_even_if_other_ranges_cover_their_bytes()
-> Result<(), Box<dyn Error>> {
    let base = legacy(1)?;
    let original = source(base.reference(), "RCNU")?;
    let entries = Entries {
        constraints: vec![
            GoalConstraint::try_from_string("guard one".to_owned())?
                .with_origin(original.range(1, 2)?),
            GoalConstraint::try_from_string("guard two".to_owned())?,
        ],
        non_goals: vec![
            NonGoal::try_from_string("omit".to_owned())?.with_origin(original.range(2, 3)?),
        ],
        decisions: vec![
            UserDecision::try_from_string("confirmed one".to_owned())?,
            UserDecision::try_from_string("confirmed two".to_owned())?
                .with_origin(original.range(3, 4)?),
        ],
    };
    let goal = GoalContract::initial(
        base.task_id(),
        grounded_draft(&original, (0, 4), entries, proof(1)?)?,
        base.created_at(),
    )
    .with_original(original)?;
    assert_eq!(goal.validate_original_binding(), Ok(()));
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert!(coverage.gaps().is_empty());
    assert!(!coverage.is_complete());
    assert_eq!(
        coverage
            .unbound_entries()
            .iter()
            .map(|entry| (entry.kind(), entry.ordinal().get()))
            .collect::<Vec<_>>(),
        vec![
            (GoalOriginalEntryKind::Constraint, 2),
            (GoalOriginalEntryKind::UserDecision, 1)
        ]
    );
    Ok(())
}

#[test]
fn overlaps_across_entry_families_do_not_double_count_or_erase_whitespace()
-> Result<(), Box<dyn Error>> {
    let base = legacy(1)?;
    let original = source(base.reference(), "R CNU")?;
    let entries = Entries {
        constraints: vec![
            GoalConstraint::try_from_string("guard".to_owned())?.with_origin(original.range(2, 4)?),
        ],
        non_goals: vec![
            NonGoal::try_from_string("omit".to_owned())?.with_origin(original.range(3, 4)?),
        ],
        decisions: vec![
            UserDecision::try_from_string("confirmed".to_owned())?
                .with_origin(original.range(4, 5)?),
        ],
    };
    let goal = GoalContract::initial(
        base.task_id(),
        grounded_draft(&original, (0, 1), entries, proof(1)?)?,
        base.created_at(),
    )
    .with_original(original)?;
    let coverage = GoalOriginalCoverage::derive_from(&goal)?;
    assert_eq!(coverage.covered_bytes(), 4);
    assert!(coverage.unbound_entries().is_empty());
    assert!(!coverage.is_complete());
    assert_eq!(
        goal.original()
            .ok_or("missing original")?
            .resolve(coverage.gaps()[0])?,
        " "
    );
    Ok(())
}

#[test]
fn all_entry_positions_through_64_keep_their_neutral_binding_identity() -> Result<(), Box<dyn Error>>
{
    let base = legacy(1)?;
    let original = source(base.reference(), "RCNU")?;
    let mut entries = Entries::default();
    for index in 0..64 {
        entries.constraints.push(
            GoalConstraint::try_from_string(format!("guard {index}"))?
                .with_origin(original.range(1, 2)?),
        );
        entries.non_goals.push(
            NonGoal::try_from_string(format!("omit {index}"))?.with_origin(original.range(2, 3)?),
        );
        entries.decisions.push(
            UserDecision::try_from_string(format!("confirmed {index}"))?
                .with_origin(original.range(3, 4)?),
        );
    }
    let goal = GoalContract::initial(
        base.task_id(),
        grounded_draft(&original, (0, 1), entries, proof(1)?)?,
        base.created_at(),
    )
    .with_original(original)?;
    assert!(GoalOriginalCoverage::derive_from(&goal)?.is_complete());
    Ok(())
}

#[test]
fn value_binding_error_and_report_debug_do_not_expose_original_or_entry_text()
-> Result<(), Box<dyn Error>> {
    let marker = "private-entry-marker";
    let base = legacy(1)?;
    let original = source(base.reference(), "private-original-marker")?;
    let foreign = source(legacy(2)?.reference(), "RCNU")?;
    let constraint =
        GoalConstraint::try_from_string(marker.to_owned())?.with_origin(foreign.range(0, 1)?);
    assert!(!format!("{constraint:?}").contains(marker));
    let candidate = GoalContract::initial(
        base.task_id(),
        grounded_draft(
            &original,
            (0, 1),
            Entries {
                constraints: vec![constraint],
                ..Entries::default()
            },
            proof(1)?,
        )?,
        base.created_at(),
    );
    let error = candidate
        .with_original(original)
        .err()
        .ok_or("expected binding rejection")?;
    assert!(!format!("{error} {error:?}").contains(marker));
    assert!(!format!("{error} {error:?}").contains("private-original-marker"));
    Ok(())
}

// No-op revision regression: only validated owner rebinding is ignored, not source content.
#[test]
fn expected_owner_rebinding_alone_is_not_a_material_goal_revision() -> Result<(), Box<dyn Error>> {
    let goal = full_goal("RCNU")?;
    let next = source(goal.next_reference()?, "RCNU")?;
    let result = goal.revise_with_original(
        grounded_draft(&next, (0, 1), entries_for(&next)?, proof(1)?)?,
        GoalRevisionReason::try_from_string("claimed change".to_owned())?,
        GoalContractTimestamp::from_unix_millis(2)?,
        next,
    );
    assert_eq!(result, Err(GoalContractRevisionFailure::NoMaterialChange));
    let next = source(goal.next_reference()?, "RCNU")?;
    let old = goal.original().ok_or("missing test original")?;
    let invalid = goal.revise_with_original(
        grounded_draft(&next, (0, 1), entries_for(old)?, proof(1)?)?,
        GoalRevisionReason::try_from_string("invalid owner claim".to_owned())?,
        GoalContractTimestamp::from_unix_millis(2)?,
        next,
    );
    assert!(matches!(
        invalid,
        Err(GoalContractRevisionFailure::InvalidOriginalBinding(
            GoalOriginalBindingError::InvalidEntryOrigin {
                error: GoalOriginalError::GoalMismatch,
                ..
            }
        ))
    ));
    Ok(())
}

#[test]
fn exact_raw_original_change_is_material_even_if_all_normalized_goal_text_is_identical()
-> Result<(), Box<dyn Error>> {
    let goal = full_goal("RCNU\r\n")?;
    let next = source(goal.next_reference()?, "RCNU\n")?;
    let revised = goal.revise_with_original(
        grounded_draft(&next, (0, 1), entries_for(&next)?, proof(1)?)?,
        GoalRevisionReason::try_from_string("changed original".to_owned())?,
        GoalContractTimestamp::from_unix_millis(2)?,
        next,
    )?;
    assert_eq!(goal.draft().objective(), revised.draft().objective());
    assert_ne!(
        goal.original().ok_or("missing original")?.as_bytes(),
        revised.original().ok_or("missing original")?.as_bytes()
    );
    Ok(())
}

#[test]
fn span_change_in_any_of_the_four_families_remains_material() -> Result<(), Box<dyn Error>> {
    let goal = full_goal("RCNU")?;
    for family in 0..4 {
        let next = source(goal.next_reference()?, "RCNU")?;
        let mut entries = entries_for(&next)?;
        let span = if family == 0 { (0, 2) } else { (0, 1) };
        match family {
            1 => {
                entries.constraints[0] = entries.constraints[0]
                    .clone()
                    .with_origin(next.range(0, 2)?)
            }
            2 => entries.non_goals[0] = entries.non_goals[0].clone().with_origin(next.range(1, 3)?),
            3 => entries.decisions[0] = entries.decisions[0].clone().with_origin(next.range(2, 4)?),
            _ => {}
        }
        let revised = goal.revise_with_original(
            grounded_draft(&next, span, entries, proof(1)?)?,
            GoalRevisionReason::try_from_string("corrected attribution".to_owned())?,
            GoalContractTimestamp::from_unix_millis(2)?,
            next,
        )?;
        assert_eq!(revised.revision().get(), 2);
        assert_eq!(revised.validate_original_binding(), Ok(()));
    }
    let base = legacy(1)?;
    let old = source(base.reference(), "RCNU")?;
    let mut partial = entries_for(&old)?;
    partial.constraints[0] = GoalConstraint::try_from_string("guard".to_owned())?;
    let partial = GoalContract::initial(
        base.task_id(),
        grounded_draft(&old, (0, 1), partial, proof(1)?)?,
        base.created_at(),
    )
    .with_original(old)?;
    let next = source(partial.next_reference()?, "RCNU")?;
    let annotated = partial.revise_with_original(
        grounded_draft(&next, (0, 1), entries_for(&next)?, proof(1)?)?,
        GoalRevisionReason::try_from_string("added missing attribution".to_owned())?,
        GoalContractTimestamp::from_unix_millis(2)?,
        next,
    )?;
    assert!(annotated.draft().constraints()[0].origin().is_some());
    Ok(())
}

#[test]
fn evidence_obligation_change_remains_material_for_the_same_original() -> Result<(), Box<dyn Error>>
{
    let goal = full_goal("RCNU")?;
    let next = source(goal.next_reference()?, "RCNU")?;
    let revised = goal.revise_with_original(
        grounded_draft(&next, (0, 1), entries_for(&next)?, proof(2)?)?,
        GoalRevisionReason::try_from_string("corrected proof obligation".to_owned())?,
        GoalContractTimestamp::from_unix_millis(2)?,
        next,
    )?;
    assert_ne!(
        goal.draft().acceptance_criteria()[0]
            .origin()
            .ok_or("missing origin")?
            .evidence_requirement(),
        revised.draft().acceptance_criteria()[0]
            .origin()
            .ok_or("missing origin")?
            .evidence_requirement()
    );
    Ok(())
}

#[test]
fn explicit_legacy_to_source_revision_is_not_treated_as_an_owner_only_noop()
-> Result<(), Box<dyn Error>> {
    let goal = legacy(1)?;
    let next = source(goal.next_reference()?, "RCNU")?;
    let revised = goal.revise_with_original(
        grounded_draft(&next, (0, 4), Entries::default(), proof(1)?)?,
        GoalRevisionReason::try_from_string("explicit source entry".to_owned())?,
        GoalContractTimestamp::from_unix_millis(2)?,
        next,
    )?;
    assert!(goal.original().is_none());
    assert!(revised.original().is_some());
    assert!(GoalOriginalCoverage::derive_from(&revised)?.is_complete());
    Ok(())
}

#[test]
fn manual_valid_owner_rebind_is_rejected_by_pair_and_both_history_boundaries()
-> Result<(), Box<dyn Error>> {
    let first = full_goal("RCNU")?;
    let next = source(first.next_reference()?, "RCNU")?;
    let manual = GoalContract::reconstruct(
        first.task_id(),
        next.goal().revision(),
        Some(first.revision()),
        Some(GoalRevisionReason::try_from_string(
            "claimed owner change".to_owned(),
        )?),
        grounded_draft(&next, (0, 1), entries_for(&next)?, proof(1)?)?,
        GoalContractTimestamp::from_unix_millis(2)?,
    )?
    .with_original(next)?;
    assert_eq!(manual.validate_original_binding(), Ok(()));
    assert_eq!(
        manual.validate_successor_of(&first),
        Err(GoalContractHistoryError::NoMaterialChange)
    );
    assert_eq!(
        GoalContractHistory::new(vec![first.clone(), manual.clone()]),
        Err(GoalContractHistoryError::NoMaterialChange)
    );
    let mut history = GoalContractHistory::new(vec![first.clone()])?;
    assert_eq!(
        history.append(manual),
        Err(GoalContractHistoryError::NoMaterialChange)
    );
    assert_eq!(history.revisions(), &[first]);
    Ok(())
}

#[test]
fn pair_validation_works_for_later_revisions_without_loading_an_initial_history()
-> Result<(), Box<dyn Error>> {
    let first = full_goal("RCNU")?;
    let second_source = source(first.next_reference()?, "RCNU\n")?;
    let second = first.revise_with_original(
        grounded_draft(
            &second_source,
            (0, 1),
            entries_for(&second_source)?,
            proof(1)?,
        )?,
        GoalRevisionReason::try_from_string("changed raw original".to_owned())?,
        GoalContractTimestamp::from_unix_millis(2)?,
        second_source,
    )?;
    let third_source = source(second.next_reference()?, "RCNU\n\n")?;
    let third = second.revise_with_original(
        grounded_draft(
            &third_source,
            (0, 1),
            entries_for(&third_source)?,
            proof(1)?,
        )?,
        GoalRevisionReason::try_from_string("changed again".to_owned())?,
        GoalContractTimestamp::from_unix_millis(3)?,
        third_source,
    )?;
    assert_eq!(third.validate_successor_of(&second), Ok(()));
    let noop_source = source(second.next_reference()?, "RCNU\n")?;
    let noop = GoalContract::reconstruct(
        second.task_id(),
        noop_source.goal().revision(),
        Some(second.revision()),
        Some(GoalRevisionReason::try_from_string(
            "claimed change".to_owned(),
        )?),
        grounded_draft(&noop_source, (0, 1), entries_for(&noop_source)?, proof(1)?)?,
        GoalContractTimestamp::from_unix_millis(3)?,
    )?
    .with_original(noop_source)?;
    assert_eq!(
        noop.validate_successor_of(&second),
        Err(GoalContractHistoryError::NoMaterialChange)
    );
    assert_eq!(
        third.validate_successor_of(&first),
        Err(GoalContractHistoryError::NonContiguous)
    );
    Ok(())
}
