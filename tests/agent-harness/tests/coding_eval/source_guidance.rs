//! Real repository/reader/context boundary; scripted choices are not a live-model quality claim.
use super::*;
use a3_application::{
    AgentActionGeneration, AgentContextCompileInput, AgentContextCompiler, AgentTurnOutcome,
    AgentTurnRejectionReason, ExecuteAgentTurn, ModelFinishReason, ModelOperationControl,
    ModelOutputChunk, ModelProvider, ModelProviderCompletion, ModelProviderFuture,
    ModelProviderRequest, ModelProviderUsage, ModelRequestTimeout, ProviderEvent,
    StagedActionFailure,
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct RepeatedReadProvider {
    provider_id: ModelProviderId,
    calls: AtomicUsize,
}

impl ModelProvider for RepeatedReadProvider {
    fn provider_id(&self) -> &ModelProviderId {
        &self.provider_id
    }

    fn stream<'a>(
        &'a self,
        request: &'a ModelProviderRequest,
        _: ModelRequestTimeout,
        _: &'a dyn ModelOperationControl,
    ) -> ModelProviderFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let title = request
                .structured_output()
                .and_then(|s| s.value().get("title"))
                .and_then(|t| t.as_str());
            let raw = match title {
                Some("A^3 SourceWork V1") => r#"{"version":1,"next":"need_evidence"}"#,
                Some("A^3 ActionChoice V1") => r#"{"version":1,"choice":"inspect_file"}"#,
                Some("A^3 ActionArguments V1") => {
                    r#"{"version":1,"parameters":{"target":{"path":"increment.py","start_line":1,"line_count":64}}}"#
                }
                _ => return Err(a3_application::ModelProviderFailure::InvalidResponse),
            };
            let events = vec![
                Ok(ProviderEvent::OutputText(
                    ModelOutputChunk::try_from_string(raw.to_owned())
                        .map_err(|_| a3_application::ModelProviderFailure::InvalidResponse)?,
                )),
                Ok(ProviderEvent::Completed(ModelProviderCompletion::new(
                    ModelFinishReason::Stop,
                    ModelProviderUsage::new(Some(100), Some(10)),
                ))),
            ];
            Ok(Box::pin(futures::stream::iter(events)) as a3_application::ProviderEventStream<'a>)
        })
    }
}

#[test]
fn real_source_guided_pack_rejects_an_already_delivered_file_before_tool_execution()
-> Result<(), Box<dyn Error>> {
    run_libsql_test(async {
        let case = small_local_bugfix();
        let fixture = CodingFixture::new(case.files).await?;
        let catalog =
            DiscoverProjectCommands.execute(fixture.project.worktree().id(), &fixture.published)?;
        let command = catalog
            .commands()
            .iter()
            .find(|c| c.kind() == DiscoveredCommandKind::Test)
            .ok_or_else(|| test_error("test command"))?;
        let step_id = TaskStepId::from_bytes(id(21));
        let spec = VerificationSpec::command(
            VerificationSpecId::from_bytes(id(22)),
            requirement("locked tests pass")?,
            command.id(),
            VerificationScope::Workspace,
        );
        let durable = DurableCodingTask::new(
            &fixture,
            case,
            AcceptanceCriterionId::from_bytes(id(20)),
            step_id,
            spec,
        )
        .await?;
        let input = AgentContextCompileInput::new(
            fixture.project.clone(),
            durable.goal.clone(),
            durable.ledger.clone(),
            step_id,
            durable.profile.clone(),
            None,
            vec![],
            vec![],
        )?;
        let source = a3_workspace::WorkspaceAgentSourceReader;
        let compiler = DeterministicAgentContextCompiler::new(
            CompileTaskLens::new(
                fixture.store.as_ref(),
                fixture.store.as_ref(),
                fixture.store.as_ref(),
            ),
            &source,
        );
        let compiled = compiler.compile(&input, &ActiveControl).await?;
        let request = a3_domain::AgentFileInspection::new(
            path("increment.py")?,
            a3_domain::AgentFileStartLine::new(1)?,
            a3_domain::AgentFileLineCount::new(64)?,
        );
        assert!(
            compiled
                .original_sources()
                .iter()
                .any(|s| s.covers(fixture.published.run().snapshot_id(), &request))
        );
        assert_eq!(
            compiled.original_sources(),
            compiler
                .compile(&input, &ActiveControl)
                .await?
                .original_sources()
        );
        let provider = RepeatedReadProvider {
            provider_id: durable.profile.provider_id().clone(),
            calls: AtomicUsize::new(0),
        };
        let tools = a3_context::DeterministicAgentReadTools::new(
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            fixture.store.as_ref(),
            &source,
        );
        let outcome = ExecuteAgentTurn::new(&compiler, &provider, &tools, fixture.store.as_ref())
            .with_patch_snapshot(&fixture.published)
            .with_action_generation(AgentActionGeneration::SourceGuided)
            .execute(&durable.run, &input, timestamp(15)?, &ActiveControl)
            .await?;
        let AgentTurnOutcome::Rejected(rejected) = outcome else {
            return Err(test_error(
                "already delivered read crossed the tool boundary",
            ));
        };
        assert_eq!(
            rejected.reason(),
            AgentTurnRejectionReason::Staged(StagedActionFailure::SourceAlreadySupplied)
        );
        assert_eq!(
            rejected.charge().repair(),
            a3_domain::AgentTurnRepairUsage::One
        );
        assert_eq!(rejected.charge().prompt_tokens().get(), 400);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
        assert_eq!(
            durable.ledger.step(step_id).map(|s| s.status()),
            Some(TaskStepStatus::InProgress)
        );
        Ok(())
    })
}
