//! Owned-loop negatives: independent repair budgets, retry exhaustion and cached live edits.
use super::*;

struct SequenceModel(
    std::sync::Mutex<std::collections::VecDeque<Result<String, AgentConversationFailure>>>,
);
impl ResearchModel for SequenceModel {
    async fn research_evidence_budget(
        &self,
        _: AgentSessionMode,
        _: Option<&str>,
    ) -> Result<usize, AgentConversationFailure> {
        Ok(1024)
    }
    async fn complete_research_decision(
        &self,
        _: AgentSessionMode,
        _: bool,
        _: &[(ModelMessageRole, String)],
        _: Option<String>,
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        self.0
            .lock()
            .map_err(|_| AgentConversationFailure::Unavailable)?
            .pop_front()
            .ok_or(AgentConversationFailure::InvalidInput)?
    }
    async fn complete_evidence_diagrams(
        &self,
        _: &[(ModelMessageRole, String)],
        _: &JobContext,
    ) -> Result<String, AgentConversationFailure> {
        Err(AgentConversationFailure::InvalidInput)
    }
}

fn owned(
    check: impl FnOnce(JobContext, JobSubmitter) -> Result<(), Box<dyn Error>> + Send + 'static,
) -> Result<(), Box<dyn Error>> {
    let (scheduler, events) =
        JobScheduler::new(JobSchedulerConfig::new(1, 2, 32)?, Arc::new(FixtureClock))?;
    let submitter = scheduler.submitter()?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    scheduler.submit(
        JobId::new(1),
        JobOwner::new(4),
        move |control: JobContext| {
            let result = check(control, submitter).map_err(|error| error.to_string());
            let success = result.is_ok();
            let _sent = send.send(result);
            if success {
                JobCompletion::Succeeded
            } else {
                JobCompletion::Failed
            }
        },
    )?;
    loop {
        let event = events
            .next_timeout(Duration::from_secs(20))?
            .ok_or("owned test timeout")?;
        if matches!(
            event.kind(),
            JobEventKind::Succeeded | JobEventKind::Failed | JobEventKind::Cancelled
        ) {
            break;
        }
    }
    receive
        .recv_timeout(Duration::from_secs(1))?
        .map_err(Into::into)
}

fn valid() -> String {
    serde_json::json!({"schema_version":4,"decision":{"kind":"answer","evidence_status":"incomplete","markdown":"Offene Evidence-Lücke.","source_refs":[],
        "note":{"goal":"Erklären","finding_kind":"hypothesis","finding":"Offen","finding_source_refs":[],"gap":"Aufrufer","next_step":"Lesen"}}}).to_string()
}

#[test]
fn single_document_repairs_and_global_retry_limits_hold_at_the_actual_decision_boundary()
-> Result<(), Box<dyn Error>> {
    owned(|control, _| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let guard = research_model::EvidenceGuard {
            project: &project,
            revisions: Vec::new(),
        };
        let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
        let model = SequenceModel(std::sync::Mutex::new(
            [Ok("{".to_owned()), Ok("{".to_owned()), Ok(valid())].into(),
        ));
        let permission = controller.begin_decision(0)?;
        let (result, diagnostics) = runtime.block_on(ask_decision(
            &model,
            AgentSessionMode::Ask,
            permission,
            &mut Vec::new(),
            &control,
            &mut controller,
            Instant::now(),
            0,
            None,
            &guard,
            false,
        ))?;
        assert_eq!(result, Err(ResearchStopReason::InvalidDecision));
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(controller.repairs_used(), 1);
        assert_eq!(controller.decisions_used(), 2);
        assert_eq!(model.0.lock().map_err(|_| "lock")?.len(), 1); // no repair of a repair

        // A citation repair's invalid output must not receive a second structural repair.
        let model = SequenceModel(std::sync::Mutex::new(
            [Ok("{".to_owned()), Ok(valid())].into(),
        ));
        controller.begin_decision(1)?;
        let (result, _) = runtime.block_on(ask_decision(
            &model,
            AgentSessionMode::Ask,
            BeginResearchDecision::FinalOnly,
            &mut Vec::new(),
            &control,
            &mut controller,
            Instant::now(),
            0,
            None,
            &guard,
            true,
        ))?;
        assert_eq!(result, Err(ResearchStopReason::InvalidDecision));
        assert_eq!(model.0.lock().map_err(|_| "lock")?.len(), 1);

        let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
        let model = SequenceModel(std::sync::Mutex::new(
            [
                Err(AgentConversationFailure::Unavailable),
                Err(AgentConversationFailure::ModelTimedOut),
                Ok(valid()),
                Err(AgentConversationFailure::Unavailable),
                Ok(valid()),
            ]
            .into(),
        ));
        for successful in [true, false] {
            let permission = controller.begin_decision(0)?;
            let (result, _) = runtime.block_on(ask_decision(
                &model,
                AgentSessionMode::Ask,
                permission,
                &mut Vec::new(),
                &control,
                &mut controller,
                Instant::now(),
                0,
                None,
                &guard,
                false,
            ))?;
            if successful {
                assert!(result.is_ok());
            } else {
                assert_eq!(result, Err(ResearchStopReason::ModelRetryLimit));
            }
        }
        assert_eq!(controller.model_retries_used(), 2);
        assert_eq!(controller.decisions_used(), 4);
        assert_eq!(controller.repairs_used(), 0);
        assert_eq!(controller.actions_used(), 0);
        assert_eq!(model.0.lock().map_err(|_| "lock")?.len(), 1);
        Ok(())
    })
}

#[test]
fn cached_revision_validation_rejects_live_edits_and_cancellation_before_model_calls()
-> Result<(), Box<dyn Error>> {
    owned(|control, submitter| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        let repository = support::TempDirectory::new()?;
        repository.git(["init", "--initial-branch=main"])?;
        let body = "def add_task():\n    return 1\n";
        repository.write("manager.py", body)?;
        let project = RepositoryInspector::new().inspect(repository.path())?;
        let revision = a3_domain::FileRevision::new(
            a3_domain::RepositoryPath::try_from_bytes(b"manager.py".to_vec())?,
            a3_domain::ContentHash::from_bytes(*blake3::hash(body.as_bytes()).as_bytes()),
        );
        let guard = research_model::EvidenceGuard {
            project: &project,
            revisions: vec![(revision, 1)],
        };
        runtime.block_on(guard.validate(&control))?;
        let late_body = format!("{}\ndef useful(): return 1\n", "#".repeat(13000));
        repository.write("late.py", &late_body)?;
        let late_guard = research_model::EvidenceGuard {
            project: &project,
            revisions: vec![(
                a3_domain::FileRevision::new(
                    a3_domain::RepositoryPath::try_from_bytes(b"late.py".to_vec())?,
                    a3_domain::ContentHash::from_bytes(
                        *blake3::hash(late_body.as_bytes()).as_bytes(),
                    ),
                ),
                2,
            )],
        };
        // Revalidation uses a known safe delivered line, not an unrelated overlong header.
        runtime.block_on(late_guard.validate(&control))?;
        repository.write("manager.py", "def add_task():\n    return 2\n")?;
        let model = SequenceModel(std::sync::Mutex::new([Ok(valid())].into()));
        let mut controller = BoundedResearchController::new(AgentResearchDepth::Standard);
        let permission = controller.begin_decision(0)?;
        assert!(matches!(
            runtime.block_on(ask_decision(
                &model,
                AgentSessionMode::Ask,
                permission,
                &mut Vec::new(),
                &control,
                &mut controller,
                Instant::now(),
                0,
                None,
                &guard,
                false
            )),
            Err(AgentSessionManagerFailure::IndexChanged)
        ));
        repository.write("manager.py", body)?;
        let outside = support::TempDirectory::new()?;
        outside.write("manager.py", body)?;
        repository.link_directory("escape", outside.path())?;
        let escape = research_model::EvidenceGuard {
            project: &project,
            revisions: vec![(
                a3_domain::FileRevision::new(
                    a3_domain::RepositoryPath::try_from_bytes(b"escape/manager.py".to_vec())?,
                    a3_domain::ContentHash::from_bytes(*blake3::hash(body.as_bytes()).as_bytes()),
                ),
                1,
            )],
        };
        assert!(runtime.block_on(escape.validate(&control)).is_err());
        submitter.cancel(JobId::new(1))?;
        assert!(runtime.block_on(guard.validate(&control)).is_err());
        assert_eq!(model.0.lock().map_err(|_| "lock")?.len(), 1);
        assert_eq!(controller.repairs_used(), 0);
        Ok(())
    })
}
