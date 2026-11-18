//! Original-bound proof obligations cannot be weakened by later plan specifications.

use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionRequirement,
    AcceptanceCriterionStatement, CriterionEvidenceRequirement, CriterionOrigin,
    CriterionVerificationMismatch, DeferredCommandVerification, DiagnosticPolicy,
    DiffInvariantMode, DiffInvariantVerification, DiscoveredCommandId, DiscoveredCommandKind,
    GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective, GoalOriginalRequest,
    MinimumTestCaseCount, PolicyResourceId, PythonIntegerTranslation, RepositoryPath,
    SuccessVerification, SymbolName, TaskId, TestCaseSelector, TestCaseSelectorName,
    VerificationMethod, VerificationRequirement, VerificationScope, VerificationSpec,
    VerificationSpecId,
};
use std::error::Error;

fn statement() -> Result<AcceptanceCriterionStatement, Box<dyn Error>> {
    Ok(AcceptanceCriterionStatement::try_from_string(
        "the existing behavioral check must pass".to_owned(),
    )?)
}

fn requirement() -> Result<VerificationRequirement, Box<dyn Error>> {
    Ok(VerificationRequirement::try_from_string(
        "verify the original required outcome".to_owned(),
    )?)
}

fn exact_selector() -> Result<TestCaseSelector, Box<dyn Error>> {
    Ok(TestCaseSelector::Exact(
        TestCaseSelectorName::try_from_string("behavior::adjust".to_owned())?,
    ))
}

fn runtime_requirement() -> Result<CriterionEvidenceRequirement, Box<dyn Error>> {
    Ok(CriterionEvidenceRequirement::ExistingTest {
        command: DiscoveredCommandId::from_bytes([2; 32]),
        selector: TestCaseSelector::All,
        minimum_cases: MinimumTestCaseCount::new(2)?,
        scope: VerificationScope::Package,
        python_integer_translation: None,
    })
}

fn test_spec(
    command: u8,
    selector: TestCaseSelector,
    count: u32,
    scope: VerificationScope,
) -> Result<VerificationSpec, Box<dyn Error>> {
    Ok(VerificationSpec::test(
        VerificationSpecId::from_bytes([3; 32]),
        requirement()?,
        DiscoveredCommandId::from_bytes([command; 32]),
        selector,
        MinimumTestCaseCount::new(count)?,
        scope,
    ))
}

#[test]
fn guarded_test_cannot_be_replaced_by_the_same_test_without_the_source_proof()
-> Result<(), Box<dyn Error>> {
    let path = RepositoryPath::try_from_bytes(b"source.py".to_vec())?;
    let function = SymbolName::try_from_string("offset".to_owned())?;
    let guard = PythonIntegerTranslation::new(path.clone(), function.clone(), 1)?;
    let command = DiscoveredCommandId::from_bytes([2; 32]);
    let minimum = MinimumTestCaseCount::new(2)?;
    let required = CriterionEvidenceRequirement::ExistingTest {
        command,
        selector: TestCaseSelector::All,
        minimum_cases: minimum,
        scope: VerificationScope::Package,
        python_integer_translation: Some(guard.clone()),
    };
    let spec = |translation| -> Result<VerificationSpec, Box<dyn Error>> {
        Ok(VerificationSpec::test_with_python_integer_translation(
            VerificationSpecId::from_bytes([3; 32]),
            requirement()?,
            command,
            TestCaseSelector::All,
            minimum,
            VerificationScope::Package,
            translation,
        ))
    };
    assert_eq!(required.validate_spec(&spec(guard)?), Ok(()));
    let mismatch = Err(CriterionVerificationMismatch::SourceRequirementMismatch);
    assert_eq!(
        required.validate_spec(&test_spec(
            2,
            TestCaseSelector::All,
            2,
            VerificationScope::Package
        )?),
        mismatch
    );
    let wrong_delta = PythonIntegerTranslation::new(path, function, -1)?;
    assert_eq!(required.validate_spec(&spec(wrong_delta)?), mismatch);
    Ok(())
}

fn path_relation(
    mode: DiffInvariantMode,
    path: &str,
) -> Result<DiffInvariantVerification, Box<dyn Error>> {
    Ok(DiffInvariantVerification::new(
        mode,
        vec![RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?],
    )?)
}

#[test]
fn legacy_constructors_do_not_fabricate_original_grounding() -> Result<(), Box<dyn Error>> {
    let id = AcceptanceCriterionId::from_bytes([1; 32]);
    assert!(
        AcceptanceCriterion::new(id, statement()?)
            .origin()
            .is_none()
    );
    assert!(
        AcceptanceCriterion::with_requirement(
            id,
            statement()?,
            AcceptanceCriterionRequirement::Should,
        )
        .origin()
        .is_none()
    );
    Ok(())
}

#[test]
fn criterion_keeps_exact_original_and_required_proof_without_replacing_its_identity()
-> Result<(), Box<dyn Error>> {
    let id = AcceptanceCriterionId::from_bytes([1; 32]);
    let criterion = AcceptanceCriterion::new(id, statement()?);
    let goal = GoalContract::initial(
        TaskId::from_bytes([9; 32]),
        GoalContractDraft::new(
            GoalObjective::try_from_string("correct the requested behavior".to_owned())?,
            vec![criterion.clone()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("run the existing behavioral tests".to_owned())?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    );
    let original = GoalOriginalRequest::try_from_bytes(
        goal.reference(),
        "  Grüße: verify behavior, not lint.\r\n"
            .as_bytes()
            .to_vec(),
    )?;
    let range = original.range(2, original.as_bytes().len())?;
    let proof = runtime_requirement()?;
    let origin = CriterionOrigin::new(range, proof.clone());
    let grounded = criterion.clone().with_origin(origin.clone());
    assert_eq!(grounded.id(), criterion.id());
    assert_eq!(grounded.statement(), criterion.statement());
    assert_eq!(grounded.requirement(), AcceptanceCriterionRequirement::Must);
    assert_eq!(grounded.origin(), Some(&origin));
    assert_eq!(origin.original_range(), range);
    assert_eq!(origin.evidence_requirement(), &proof);
    assert_eq!(
        original.resolve(origin.original_range())?,
        "Grüße: verify behavior, not lint.\r\n"
    );
    assert!(
        criterion.origin().is_none(),
        "legacy value must remain unchanged"
    );
    Ok(())
}

#[test]
fn runtime_test_obligation_rejects_lint_diff_exit_only_deferred_confirmation_and_legacy()
-> Result<(), Box<dyn Error>> {
    let proof = runtime_requirement()?;
    let id = VerificationSpecId::from_bytes([3; 32]);
    let command = DiscoveredCommandId::from_bytes([2; 32]);
    let substitutions = vec![
        VerificationSpec::command(id, requirement()?, command, VerificationScope::Workspace),
        VerificationSpec::diagnostic(
            id,
            requirement()?,
            command,
            DiagnosticPolicy::NoWarnings,
            VerificationScope::Workspace,
        ),
        VerificationSpec::diff_invariant(
            id,
            requirement()?,
            path_relation(DiffInvariantMode::ExactPaths, "subject.py")?,
        ),
        VerificationSpec::deferred_command(
            id,
            requirement()?,
            DeferredCommandVerification::new(
                vec![DiscoveredCommandKind::Test],
                VerificationScope::Workspace,
            )?,
        ),
        VerificationSpec::user_confirm(id, requirement()?, PolicyResourceId::from_bytes([8; 32])),
        VerificationSpec::new(id, VerificationMethod::Test, requirement()?),
    ];
    for spec in substitutions {
        assert_eq!(
            proof.validate_spec(&spec),
            Err(CriterionVerificationMismatch::EvidenceKindMismatch)
        );
    }
    Ok(())
}

#[test]
fn replanning_cannot_change_command_selector_minimum_or_reduce_scope() -> Result<(), Box<dyn Error>>
{
    let proof = runtime_requirement()?;
    for (spec, mismatch) in [
        (
            test_spec(4, TestCaseSelector::All, 2, VerificationScope::Package)?,
            CriterionVerificationMismatch::CommandMismatch,
        ),
        (
            test_spec(2, exact_selector()?, 2, VerificationScope::Package)?,
            CriterionVerificationMismatch::SelectorMismatch,
        ),
        (
            test_spec(2, TestCaseSelector::All, 1, VerificationScope::Package)?,
            CriterionVerificationMismatch::MinimumCasesTooSmall,
        ),
        (
            test_spec(2, TestCaseSelector::All, 2, VerificationScope::Targeted)?,
            CriterionVerificationMismatch::ScopeTooNarrow,
        ),
    ] {
        assert_eq!(proof.validate_spec(&spec), Err(mismatch));
    }
    // Both directions of selector replacement are rejected; All is not treated
    // as an implicit semantic superset of one exact selected case.
    let exact = CriterionEvidenceRequirement::ExistingTest {
        command: DiscoveredCommandId::from_bytes([2; 32]),
        selector: exact_selector()?,
        minimum_cases: MinimumTestCaseCount::new(1)?,
        scope: VerificationScope::Package,
        python_integer_translation: None,
    };
    assert_eq!(
        exact.validate_spec(&test_spec(
            2,
            TestCaseSelector::All,
            2,
            VerificationScope::Package
        )?),
        Err(CriterionVerificationMismatch::SelectorMismatch)
    );
    Ok(())
}

#[test]
fn equivalent_or_stronger_specs_may_use_new_core_owned_identities() -> Result<(), Box<dyn Error>> {
    let proof = runtime_requirement()?;
    let exact = test_spec(2, TestCaseSelector::All, 2, VerificationScope::Package)?;
    assert_eq!(proof.validate_spec(&exact), Ok(()));
    let stronger = test_spec(2, TestCaseSelector::All, 3, VerificationScope::Workspace)?
        .reidentified(VerificationSpecId::from_bytes([7; 32]));
    assert_eq!(proof.validate_spec(&stronger), Ok(()));
    Ok(())
}

#[test]
fn change_only_accepts_its_exact_relation_but_not_an_empty_or_different_effect()
-> Result<(), Box<dyn Error>> {
    let required = path_relation(DiffInvariantMode::ExactPaths, "README.md")?;
    let proof = CriterionEvidenceRequirement::ChangeOnly(required.clone());
    let id = VerificationSpecId::from_bytes([3; 32]);
    assert_eq!(
        proof.validate_spec(&VerificationSpec::diff_invariant(
            id,
            requirement()?,
            required
        )),
        Ok(())
    );
    for replacement in [
        path_relation(DiffInvariantMode::OnlyPaths, "README.md")?,
        path_relation(DiffInvariantMode::ExactPaths, "other.md")?,
        DiffInvariantVerification::new(DiffInvariantMode::NoChanges, Vec::new())?,
        DiffInvariantVerification::new(DiffInvariantMode::NonEmptyChanges, Vec::new())?,
    ] {
        assert_eq!(
            proof.validate_spec(&VerificationSpec::diff_invariant(
                id,
                requirement()?,
                replacement
            )),
            Err(CriterionVerificationMismatch::DiffInvariantMismatch)
        );
    }
    assert_eq!(
        proof.validate_spec(&test_spec(
            2,
            exact_selector()?,
            2,
            VerificationScope::Package
        )?),
        Err(CriterionVerificationMismatch::EvidenceKindMismatch)
    );
    Ok(())
}

#[test]
fn missing_runtime_recipe_never_falls_back_to_test_command_or_diff() -> Result<(), Box<dyn Error>> {
    let proof = CriterionEvidenceRequirement::RuntimeNeedsRecipe;
    let id = VerificationSpecId::from_bytes([3; 32]);
    for proposed in [
        test_spec(2, exact_selector()?, 2, VerificationScope::Workspace)?,
        VerificationSpec::command(
            id,
            requirement()?,
            DiscoveredCommandId::from_bytes([2; 32]),
            VerificationScope::Workspace,
        ),
        VerificationSpec::diff_invariant(
            id,
            requirement()?,
            path_relation(DiffInvariantMode::ExactPaths, "server.py")?,
        ),
    ] {
        assert_eq!(
            proof.validate_spec(&proposed),
            Err(CriterionVerificationMismatch::RuntimeRecipeUnavailable)
        );
    }
    Ok(())
}
