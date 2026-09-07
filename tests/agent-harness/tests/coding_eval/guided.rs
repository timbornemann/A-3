//! Real patch, durable receipt, model work decision, real process and acceptance.
use super::*;
use a3_application::{
    AgentActionGeneration, AgentContextCompileInput, AgentTurnOutcome, ExecuteAgentTurn,
    LoadAgentExecutionCheckpoint, ModelCancellationFuture, ModelFinishReason,
    ModelOperationControl, ModelOutputChunk, ModelProviderCompletion, ModelProviderUsage,
    ProviderEvent,
};
use a3_context::DeterministicAgentReadTools;
use a3_model_provider_contract_tests::{StubModelProvider, StubModelProviderBehavior};

pub(super) enum VerificationSource {
    FixtureDriver,
    AfterChangeDecision,
}

impl ModelOperationControl for ActiveControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn cancelled(&self) -> ModelCancellationFuture<'_> {
        Box::pin(std::future::pending())
    }
}

pub(super) async fn select_verification(
    fixture: &CodingFixture,
    durable: &mut DurableCodingTask,
    compiler: &DeterministicAgentContextCompiler<'_>,
    published: &PublishedIndex,
) -> Result<AgentAction, Box<dyn Error>> {
    let step_id = durable
        .ledger
        .steps()
        .next()
        .ok_or_else(|| test_error("step missing"))?
        .definition()
        .id();
    let receipt = LoadAgentExecutionCheckpoint::new(fixture.store.as_ref(), fixture.store.as_ref())
        .execute(&fixture.project, &durable.run, &ActiveControl)
        .await?
        .ok_or_else(|| test_error("actual patch receipt missing"))?;
    let input = AgentContextCompileInput::new(
        fixture.project.clone(),
        durable.goal.clone(),
        durable.ledger.clone(),
        step_id,
        durable.profile.clone(),
        None,
        Vec::new(),
        Vec::new(),
    )?
    .with_execution_checkpoint(receipt)?;
    let provider = StubModelProvider::new(
        durable.profile.provider_id().clone(),
        StubModelProviderBehavior::Events(vec![
            ProviderEvent::OutputText(ModelOutputChunk::try_from_string(
                r#"{"version":1,"next":"verify"}"#.to_owned(),
            )?),
            ProviderEvent::Completed(ModelProviderCompletion::new(
                ModelFinishReason::Stop,
                ModelProviderUsage::new(Some(100), Some(10)),
            )),
        ]),
    );
    let source = a3_workspace::WorkspaceAgentSourceReader;
    let tools = DeterministicAgentReadTools::new(
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        fixture.store.as_ref(),
        &source,
    );
    let outcome = ExecuteAgentTurn::new(compiler, &provider, &tools, fixture.store.as_ref())
        .with_patch_snapshot(published)
        .with_action_generation(AgentActionGeneration::ReviewThenSelect)
        .execute(&durable.run, &input, timestamp(25)?, &ActiveControl)
        .await?;
    let AgentTurnOutcome::Executed(execution) = &outcome else {
        return Err(
            std::io::Error::other(format!("guided verification rejected: {outcome:?}")).into(),
        );
    };
    let AgentAction::Run(command) = execution.action() else {
        return Err(test_error("guided decision did not request a run"));
    };
    let expected = a3_application::RequestAgentFinish
        .verification_command(
            input
                .task_ledger()
                .step(step_id)
                .ok_or_else(|| test_error("step missing"))?,
        )
        .ok_or_else(|| test_error("planned command missing"))?;
    assert_eq!(command, &expected);
    assert_eq!(provider.calls()?.len(), 1);
    assert!(execution.tool_result().is_none());
    assert_eq!(
        durable.ledger.step(step_id).map(|step| step.status()),
        Some(TaskStepStatus::InProgress)
    );
    let action = execution.action().clone();
    let sequence = durable.run.last_event_sequence();
    let event = outcome.record(
        &mut durable.run,
        RunEventId::from_bytes(id(110)),
        timestamp(25)?,
    )?;
    AppendRunEvent::new(fixture.store.as_ref())
        .execute(&fixture.project, sequence, &durable.run, &event)
        .await?;
    Ok(action)
}

#[test]
fn real_guided_post_patch_request_verifies_one_and_two_file_changes() -> Result<(), Box<dyn Error>>
{
    run_libsql_test(async {
        for case in [small_local_bugfix(), two_module_change()] {
            let result =
                evaluate_case_with_verification(case, VerificationSource::AfterChangeDecision)
                    .await?;
            assert_eq!(result.final_state, "done");
            assert!(
                result.goal
                    && result.step
                    && result.patch
                    && result.evidence
                    && result.verification
                    && result.foreign_change_preserved
            );
        }
        Ok(())
    })
}
