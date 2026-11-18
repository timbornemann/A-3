//! Exact file-addition scopes retain their matching verification target.

use a3_domain::*;
use std::error::Error;
type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn path(value: &str) -> TestResult<RepositoryPath> {
    Ok(RepositoryPath::try_from_bytes(value.as_bytes().to_vec())?)
}

fn definition(spec: VerificationSpec) -> TestResult<TaskStepDefinition> {
    Ok(TaskStepDefinition::new(
        TaskStepId::from_bytes([1; 32]),
        None,
        TaskStepOutcome::try_from_string("Create the selected file".to_owned())?,
        TaskStepRationale::try_from_string("A bounded explicit operation".to_owned())?,
        Vec::new(),
        vec![ExpectedTaskEvidence::try_from_string(
            "Exact diff".to_owned(),
        )?],
        spec,
    )?)
}

fn diff(mode: DiffInvariantMode, paths: Vec<RepositoryPath>) -> TestResult<VerificationSpec> {
    Ok(VerificationSpec::diff_invariant(
        VerificationSpecId::from_bytes([2; 32]),
        VerificationRequirement::try_from_string("Check the target scope".to_owned())?,
        DiffInvariantVerification::new(mode, paths)?,
    ))
}

#[test]
fn planned_addition_requires_the_identical_singleton_exact_diff_scope() -> TestResult {
    let addition = PlannedFileAddition::new(path("src/ü.py")?);
    let step = definition(diff(
        DiffInvariantMode::ExactPaths,
        vec![addition.target().clone()],
    )?)?;
    assert!(step.planned_file_addition().is_none());
    let step = step.with_planned_file_addition(addition.clone())?;
    assert_eq!(step.planned_file_addition(), Some(&addition));
    for spec in [
        diff(DiffInvariantMode::ExactPaths, vec![path("other.py")?])?,
        diff(
            DiffInvariantMode::ExactPaths,
            vec![addition.target().clone(), path("other.py")?],
        )?,
        diff(
            DiffInvariantMode::OnlyPaths,
            vec![addition.target().clone()],
        )?,
        diff(DiffInvariantMode::NonEmptyChanges, Vec::new())?,
        VerificationSpec::user_confirm(
            VerificationSpecId::from_bytes([3; 32]),
            VerificationRequirement::try_from_string("Confirm observed behavior".to_owned())?,
            PolicyResourceId::from_bytes([4; 32]),
        ),
    ] {
        assert_eq!(
            definition(spec)?.with_planned_file_addition(addition.clone()),
            Err(TaskStepDefinitionError::InvalidPlannedFileAdditionScope)
        );
    }
    assert_ne!(
        addition.scope_digest(),
        PlannedFileAddition::new(path("other.py")?).scope_digest()
    );
    Ok(())
}
