//! A step's progress role cannot become a global criterion proof by duplication or a setter.
use a3_domain::{
    AcceptanceCriterionId, ExpectedTaskEvidence, TaskStepDefinition, TaskStepDefinitionError,
    TaskStepId, TaskStepOutcome, TaskStepRationale, VerificationMethod, VerificationRequirement,
    VerificationSpec, VerificationSpecId,
};
use std::error::Error;
fn id(n: u8) -> AcceptanceCriterionId {
    AcceptanceCriterionId::from_bytes([n; 32])
}
fn definition() -> Result<TaskStepDefinition, Box<dyn Error>> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([1; 32]),
        None,
        TaskStepOutcome::try_from_string("prepare an implementation slice".to_owned())?,
        TaskStepRationale::try_from_string("advance the original requirement".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "the declared local check passes".to_owned(),
        )?],
        VerificationSpec::new(
            VerificationSpecId::from_bytes([2; 32]),
            VerificationMethod::DiffInvariant,
            VerificationRequirement::try_from_string("check the local change".to_owned())?,
        ),
    )?)
}
#[test]
fn legacy_empty_and_proof_setter_rules_are_unchanged() -> Result<(), Box<dyn Error>> {
    let base = definition()?;
    assert!(base.addressed_criteria().is_empty());
    assert!(base.acceptance_criteria().is_empty());
    assert_eq!(
        base.clone().with_acceptance_criteria(Vec::new()),
        Err(TaskStepDefinitionError::InvalidAcceptanceCriterionCount(0))
    );
    assert_eq!(
        base.clone().with_acceptance_criteria(vec![id(1); 65]),
        Err(TaskStepDefinitionError::InvalidAcceptanceCriterionCount(65))
    );
    assert_eq!(
        base.clone().with_acceptance_criteria(vec![id(1), id(1)]),
        Err(TaskStepDefinitionError::DuplicateAcceptanceCriterion)
    );
    let proof = base.with_acceptance_criteria(vec![id(3), id(1)])?;
    assert_eq!(proof.acceptance_criteria(), &[id(1), id(3)]);
    assert!(proof.addressed_criteria().is_empty());
    Ok(())
}
#[test]
fn progress_and_proof_roles_are_canonical_disjoint_and_distinct() -> Result<(), Box<dyn Error>> {
    let mixed = definition()?.with_criterion_coverage(vec![id(4), id(1)], vec![id(3), id(2)])?;
    assert_eq!(mixed.addressed_criteria(), &[id(1), id(4)]);
    assert_eq!(mixed.acceptance_criteria(), &[id(2), id(3)]);
    let progress = definition()?.with_criterion_coverage(vec![id(1)], Vec::new())?;
    assert!(progress.acceptance_criteria().is_empty());
    assert_eq!(progress.addressed_criteria(), &[id(1)]);
    assert_ne!(
        progress,
        definition()?.with_acceptance_criteria(vec![id(1)])?
    );
    Ok(())
}
#[test]
fn empty_duplicate_overlap_and_union_overflow_are_rejected() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        definition()?.with_criterion_coverage(Vec::new(), Vec::new()),
        Err(TaskStepDefinitionError::InvalidCriterionCoverageCount(0))
    );
    for (a, p) in [
        (vec![id(1), id(1)], Vec::new()),
        (Vec::new(), vec![id(1), id(1)]),
    ] {
        assert_eq!(
            definition()?.with_criterion_coverage(a, p),
            Err(TaskStepDefinitionError::DuplicateAcceptanceCriterion)
        );
    }
    assert_eq!(
        definition()?.with_criterion_coverage(vec![id(1)], vec![id(1)]),
        Err(TaskStepDefinitionError::OverlappingCriterionRoles)
    );
    assert_eq!(
        definition()?.with_criterion_coverage((1..=64).map(id).collect(), vec![id(65)]),
        Err(TaskStepDefinitionError::InvalidCriterionCoverageCount(65))
    );
    Ok(())
}
#[test]
fn old_proof_setter_cannot_overlap_or_erase_existing_progress() -> Result<(), Box<dyn Error>> {
    let mixed = definition()?.with_criterion_coverage(vec![id(1)], vec![id(2)])?;
    assert_eq!(
        mixed.clone().with_acceptance_criteria(vec![id(1)]),
        Err(TaskStepDefinitionError::OverlappingCriterionRoles)
    );
    assert_eq!(
        mixed.clone().with_acceptance_criteria(vec![id(1), id(1)]),
        Err(TaskStepDefinitionError::DuplicateAcceptanceCriterion)
    );
    assert_eq!(
        mixed.clone().with_acceptance_criteria(Vec::new()),
        Err(TaskStepDefinitionError::InvalidAcceptanceCriterionCount(0))
    );
    let changed = mixed.with_acceptance_criteria(vec![id(3)])?;
    assert_eq!(changed.addressed_criteria(), &[id(1)]);
    assert_eq!(changed.acceptance_criteria(), &[id(3)]);
    let full = definition()?.with_criterion_coverage((1..=64).map(id).collect(), Vec::new())?;
    assert_eq!(
        full.with_acceptance_criteria(vec![id(65)]),
        Err(TaskStepDefinitionError::InvalidCriterionCoverageCount(65))
    );
    Ok(())
}
#[test]
fn separate_steps_may_address_and_later_prove_the_same_original_criterion()
-> Result<(), Box<dyn Error>> {
    let first = definition()?.with_criterion_coverage(vec![id(1)], Vec::new())?;
    let second = definition()?.with_acceptance_criteria(vec![id(1)])?;
    assert_eq!(first.addressed_criteria(), second.acceptance_criteria());
    Ok(())
}
