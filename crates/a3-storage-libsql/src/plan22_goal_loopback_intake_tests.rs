//! Actual candidate Goal and Intake persistence for exact HTTP proof obligations.

use super::*;
use crate::goal_contract_repository::{self, GoalContractRepositoryError};
use a3_application::{MaterializeOriginalTask, TaskLedgerStore};
use a3_domain::{
    ContentHash, ExpectedTaskEvidence, LoopbackExpectedBody, LoopbackHeaderChecks,
    LoopbackHttpCheck, LoopbackHttpRecipe, LoopbackMethod, LoopbackProbe, LoopbackStart,
    TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, VerificationRequirement,
    VerificationSpec, VerificationSpecId,
};
use std::num::NonZeroU16;

const REQUEST: &str = "Serve a bounded text response and reject POST on the same route.\n";

#[test]
fn loopback_intake_review_promotion_and_cold_goal_restore_are_exact() -> TestResult {
    roundtrip_recipe(REQUEST, recipe()?)
}

#[test]
fn loopback_startup_coverage_restores_the_whole_original_obligation() -> TestResult {
    let original = "Serve the exact HTTP response with explicit and default startup. \
        Invalid port arguments and startup on an occupied explicit port must fail.\n";
    roundtrip_recipe(original, recipe()?.with_startup_coverage(0)?)
}

fn roundtrip_recipe(request: &str, recipe: LoopbackHttpRecipe) -> TestResult {
    crate::run_native_libsql_test(async {
        let fixture = Fixture::new()?;
        let task = TaskId::from_bytes([19; 32]);
        let original = GoalOriginalRequest::try_from_bytes(
            GoalContractReference::initial(task),
            request.as_bytes().to_vec(),
        )?;
        let range = original.range(0, request.len())?;
        let spec = VerificationSpec::loopback_http(
            VerificationSpecId::from_bytes([22; 32]),
            VerificationRequirement::try_from_string(
                "Verify the complete loopback recipe".to_owned(),
            )?,
            recipe.clone(),
        );
        let requirement = CriterionEvidenceRequirement::LoopbackHttp(recipe);
        let criterion = criterion(20, request.trim_end())?
            .with_origin(CriterionOrigin::new(range, requirement.clone()));
        let draft = GoalContractDraft::new(
            GoalObjective::try_from_string(request.trim_end().to_owned())?,
            vec![criterion],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("Run the exact loopback checks".to_owned())?,
        )?;
        let goal = GoalContract::initial(task, draft, GoalContractTimestamp::from_unix_millis(10)?)
            .with_original(original)?;
        let store = captured(&fixture, &goal).await?;
        let proposal = GoalIntakeReviewBatchProposal::new(
            range,
            vec![finding(
                GoalIntakeFindingKind::Outcome {
                    requirement: AcceptanceCriterionRequirement::Must,
                    evidence_requirement: requirement.clone(),
                },
                range,
                request.trim_end(),
            )?],
            Vec::new(),
        )?;
        let review = RecordGoalIntakeReview::new(&store)
            .execute(
                &fixture.project,
                task,
                GoalIntakeStoreVersion::INITIAL,
                None,
                proposal,
            )
            .await?;
        assert_pending(&store, &fixture, &goal).await?;
        drop(store);

        let cold = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        assert_eq!(
            cold.load_goal_intake_review(&fixture.project, task).await?,
            Some(review.clone())
        );
        cold.promote_goal_intake(
            &fixture.project,
            task,
            GoalIntakeStoreVersion::INITIAL,
            &goal,
        )
        .await?;
        drop(cold);

        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let restored = reopened
            .load_current_goal_contract(&fixture.project, task)
            .await?
            .ok_or("missing promoted HTTP Goal")?;
        let [criterion] = restored.draft().acceptance_criteria() else {
            return Err("expected one restored HTTP criterion".into());
        };
        assert_eq!(
            criterion.origin(),
            Some(&CriterionOrigin::new(range, requirement))
        );
        assert_eq!(restored, goal);
        assert_eq!(
            reopened
                .load_goal_intake_review(&fixture.project, task)
                .await?,
            Some(review)
        );
        let step = TaskStepId::from_bytes([23; 32]);
        let definition = TaskStepDefinition::new(
            step,
            None,
            TaskStepOutcome::try_from_string(
                "Implement the requested loopback behavior".to_owned(),
            )?,
            TaskStepRationale::try_from_string("Prove the original obligation".to_owned())?,
            Vec::new(),
            vec![ExpectedTaskEvidence::try_from_string(
                "Owned runtime observations".to_owned(),
            )?],
            spec.clone(),
        )?
        .with_criterion_coverage(Vec::new(), vec![criterion.id()])?;
        MaterializeOriginalTask::new(&reopened, &reopened)
            .execute(&fixture.project, &restored, vec![definition])
            .await?;
        drop(reopened);
        let reopened = LibsqlKnowledgeStore::open_plan22_candidate(&fixture.layout).await?;
        let stored = reopened
            .load_task_ledger(&fixture.project, task)
            .await?
            .ok_or("missing reopened loopback ledger")?;
        let definition = stored
            .ledger()
            .step(step)
            .ok_or("missing reopened loopback step")?
            .definition();
        assert_eq!(definition.verification_spec(), &spec);
        assert_eq!(definition.acceptance_criteria(), &[criterion.id()]);

        let db = fixture.candidate_database().await?;
        let connection = db.connection_for_operation().await?;
        let worktree = fixture.project.worktree().id();
        // Exercise the decoder against corruption deliberately injected into the owned fixture.
        connection
            .execute_batch(
                "DROP TRIGGER goal_intake_review_findings_update_guard;
          UPDATE goal_intake_review_findings SET loopback_http_recipe=zeroblob(65537)
          WHERE loopback_http_recipe IS NOT NULL",
            )
            .await?;
        assert!(matches!(
            goal_contract_repository::intake_review::load(&connection, worktree, task).await,
            Err(GoalContractRepositoryError::ResourceLimit)
        ));
        connection
            .execute(
                "UPDATE goal_intake_review_findings SET loopback_http_recipe=zeroblob(52)
          WHERE loopback_http_recipe IS NOT NULL",
                (),
            )
            .await?;
        assert!(matches!(
            goal_contract_repository::intake_review::load(&connection, worktree, task).await,
            Err(GoalContractRepositoryError::InvalidStoredData)
        ));
        connection.execute_batch("DROP TRIGGER goal_criterion_origins_update_guard;
          UPDATE goal_criterion_origins SET loopback_http_recipe=zeroblob(length(loopback_http_recipe))
          WHERE loopback_http_recipe IS NOT NULL").await?;
        assert!(matches!(
            goal_contract_repository::load_current(&connection, worktree, task).await,
            Err(GoalContractRepositoryError::InvalidStoredData)
        ));
        drop(connection);
        drop(db);
        drop(reopened);
        fixture.cleanup()
    })
}

fn recipe() -> TestResult<LoopbackHttpRecipe> {
    let body = b"ready\n";
    Ok(LoopbackHttpRecipe::new(
        RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
        LoopbackStart::Explicit,
        NonZeroU16::new(18_765).ok_or("invalid test port")?,
        vec![
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Get, "/health".to_owned())?,
                200,
                LoopbackExpectedBody::Exact {
                    hash: ContentHash::from_bytes(*blake3::hash(body).as_bytes()),
                    bytes: u32::try_from(body.len())?,
                },
                LoopbackHeaderChecks {
                    plain_utf8: true,
                    allow_get: false,
                },
            )?,
            LoopbackHttpCheck::new(
                LoopbackProbe::new(LoopbackMethod::Post, "/health".to_owned())?,
                405,
                LoopbackExpectedBody::AnyBounded,
                LoopbackHeaderChecks {
                    plain_utf8: false,
                    allow_get: true,
                },
            )?,
        ],
    )?)
}
