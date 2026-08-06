//! H8 deterministic run-memory projection contracts.

use a3_domain::{
    AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, AgentControllerState,
    AgentRun, AgentRunId, AgentRunIdentity, AgentRunMaterializedState, AgentRunTimestamp,
    AgentRunTiming, Centrality, Confidence, ContentHash, ExpectedTaskEvidence, FileRevision,
    GoalContract, GoalContractDraft, GoalContractTimestamp, GoalObjective, GraphSymbol,
    IndexLanguage, IndexPublication, IndexRunId, IndexRunRecord, IndexRunSequence, IndexRunStatus,
    LinkedGraph, LocalSymbolId, ModelProfileId, ModelProfileReference, ModelProfileVersion,
    ModuleCardClaimId, ModuleCardEvidenceId, ModuleClaimPolarity, ModuleClaimPredicate,
    ModuleClaimStatement, ModuleId, ModuleKind, ModuleMembership, ModuleMembershipEvidence,
    ModulePolicyVersion, ModuleProjection, ModuleRoot, ModuleSymbolSet, OpenRunIssueKind,
    ParsedSymbol, PublishedIndex, RankProjection, RankScore, RankingPolicyVersion, RepositoryCard,
    RepositoryModule, RepositoryPath, ResolvedModuleCardEvidence, RunEventSequence,
    RunMemoryCheckpoint, RunMemoryCompileError, SnapshotId, SourcePosition, SourceRange,
    StepDependency, StepVerification, StepVerificationId, StepVerificationOutcome,
    SuccessVerification, SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals,
    TaskEvidenceId, TaskId, TaskLedger, TaskLedgerRevision, TaskLedgerTimestamp, TaskLensClaim,
    TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, TaskStepResultSummary,
    VerificationFailureSummary, VerificationMethod, VerificationRequirement, VerificationSpec,
    VerificationSpecId, VerifiedClaimKind, VerifiedClaimStatus,
};
use std::error::Error;

#[test]
fn repeated_compaction_retains_goal_sources_hypotheses_and_open_failures()
-> Result<(), Box<dyn Error>> {
    let goal = goal()?;
    let first_id = TaskStepId::from_bytes([1; 32]);
    let second_id = TaskStepId::from_bytes([2; 32]);
    let run_id = AgentRunId::from_bytes([3; 32]);
    let mut ledger = TaskLedger::new(
        goal.reference(),
        vec![
            step(first_id, 1, Vec::new())?,
            step(second_id, 2, vec![StepDependency::new(first_id)])?,
        ],
        timestamp(1)?,
    )?;
    complete_step(&mut ledger, first_id, 1, run_id, 2, 10, 11)?;

    let published = published_index()?;
    let index_run_id = published.run().id();
    let snapshot_id = published.run().snapshot_id();
    let claims = claims(index_run_id, snapshot_id)?;
    let first_run = run(&goal, &ledger, snapshot_id, 3)?;
    let first =
        RunMemoryCheckpoint::compile(&goal, &ledger, &first_run, &published, claims.clone())?;
    assert_eq!(first.goal_contract(), goal.reference());
    assert_eq!(first.step_results().len(), 1);
    assert_eq!(first.claims().len(), 3);
    assert_eq!(first.open_hypotheses().count(), 2);
    assert_eq!(first.excluded_stale_claims(), 1);
    assert!(
        first.claims().iter().any(|claim| {
            claim.claim().source_index_run_id() == IndexRunId::from_bytes([99; 32])
        })
    );
    assert!(first.open_issues().is_empty());
    assert_eq!(
        first.step_results()[0].evidence_ids(),
        &[
            TaskEvidenceId::from_bytes([10; 32]),
            TaskEvidenceId::from_bytes([11; 32])
        ]
    );

    fail_verification(&mut ledger, second_id, 2, run_id, 5, 12, 13)?;
    let second_run = run(&goal, &ledger, snapshot_id, 8)?;
    let second =
        RunMemoryCheckpoint::compile(&goal, &ledger, &second_run, &published, claims.clone())?;
    let mut repeated = None;
    for _ in 0..64 {
        let checkpoint =
            RunMemoryCheckpoint::compile(&goal, &ledger, &second_run, &published, claims.clone())?;
        assert_eq!(checkpoint.goal_contract(), goal.reference());
        assert_eq!(checkpoint.open_hypotheses().count(), 2);
        assert_eq!(checkpoint.open_issues().len(), 1);
        repeated = Some(checkpoint);
    }
    let repeated = repeated.ok_or_else(|| std::io::Error::other("long-run fixture was empty"))?;

    assert_eq!(second.digest(), repeated.digest());
    assert_ne!(first.digest(), second.digest());
    assert_eq!(second.goal_contract(), goal.reference());
    assert_eq!(second.step_results().len(), 2);
    assert_eq!(second.open_hypotheses().count(), 2);
    assert_eq!(second.open_issues().len(), 1);
    assert_eq!(
        second.open_issues()[0].kind(),
        OpenRunIssueKind::VerificationFailed
    );
    assert_eq!(second.open_issues()[0].step_id(), second_id);
    assert_eq!(second.through_event_sequence(), RunEventSequence::new(8)?);
    assert_eq!(second_run.last_event_sequence(), RunEventSequence::new(8)?);
    assert!(second.step_results().iter().all(|result| {
        result.source().attempt_number().get() > 0
            && result.source().run_id() == run_id
            && result.summary().is_some()
    }));
    assert_eq!(second.step_results()[0].source().step_id(), first_id);
    assert_eq!(second.step_results()[1].source().step_id(), second_id);
    Ok(())
}

#[test]
fn compaction_rejects_mismatched_run_and_snapshot_anchors() -> Result<(), Box<dyn Error>> {
    let goal = goal()?;
    let ledger = TaskLedger::new(
        goal.reference(),
        vec![step(TaskStepId::from_bytes([1; 32]), 1, Vec::new())?],
        timestamp(1)?,
    )?;
    let published = published_index()?;
    let mismatched_ledger_run = AgentRun::reconstruct(
        AgentRunIdentity::new(
            AgentRunId::from_bytes([3; 32]),
            goal.reference(),
            TaskLedgerRevision::new(2)?,
            None,
        ),
        AgentRunMaterializedState::new(
            AgentControllerState::Verify,
            RunEventSequence::new(1)?,
            published.run().snapshot_id(),
        ),
        AgentRunTiming::new(
            AgentRunTimestamp::from_unix_millis(1)?,
            AgentRunTimestamp::from_unix_millis(1)?,
        ),
    )?;
    assert!(matches!(
        RunMemoryCheckpoint::compile(
            &goal,
            &ledger,
            &mismatched_ledger_run,
            &published,
            Vec::new()
        ),
        Err(RunMemoryCompileError::RunAnchorMismatch)
    ));

    let mismatched_snapshot_run = run(&goal, &ledger, SnapshotId::from_bytes([99; 32]), 1)?;
    assert!(matches!(
        RunMemoryCheckpoint::compile(
            &goal,
            &ledger,
            &mismatched_snapshot_run,
            &published,
            Vec::new()
        ),
        Err(RunMemoryCompileError::SnapshotMismatch)
    ));
    Ok(())
}

fn goal() -> Result<GoalContract, Box<dyn Error>> {
    Ok(GoalContract::initial(
        TaskId::from_bytes([90; 32]),
        GoalContractDraft::new(
            GoalObjective::try_from_string("retain the original goal".to_owned())?,
            vec![AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([91; 32]),
                AcceptanceCriterionStatement::try_from_string(
                    "sources survive compaction".to_owned(),
                )?,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("verify run memory".to_owned())?,
        )?,
        GoalContractTimestamp::from_unix_millis(1)?,
    ))
}

fn run(
    goal: &GoalContract,
    ledger: &TaskLedger,
    snapshot_id: SnapshotId,
    sequence: u64,
) -> Result<AgentRun, Box<dyn Error>> {
    Ok(AgentRun::reconstruct(
        AgentRunIdentity::new(
            AgentRunId::from_bytes([3; 32]),
            goal.reference(),
            ledger.revision(),
            Some(ModelProfileReference::new(
                ModelProfileId::from_bytes([4; 32]),
                ModelProfileVersion::V1,
            )),
        ),
        AgentRunMaterializedState::new(
            AgentControllerState::Verify,
            RunEventSequence::new(sequence)?,
            snapshot_id,
        ),
        AgentRunTiming::new(
            AgentRunTimestamp::from_unix_millis(1)?,
            AgentRunTimestamp::from_unix_millis(sequence)?,
        ),
    )?)
}

fn step(
    id: TaskStepId,
    spec: u8,
    dependencies: Vec<StepDependency>,
) -> Result<TaskStepDefinition, Box<dyn Error>> {
    Ok(TaskStepDefinition::new(
        id,
        None,
        TaskStepOutcome::try_from_string(format!("produce step {spec}"))?,
        TaskStepRationale::try_from_string(format!("retain step {spec}"))?,
        dependencies,
        vec![ExpectedTaskEvidence::try_from_string(format!(
            "evidence {spec}"
        ))?],
        VerificationSpec::new(
            VerificationSpecId::from_bytes([spec; 32]),
            VerificationMethod::Test,
            VerificationRequirement::try_from_string(format!("verify {spec}"))?,
        ),
    )?)
}

fn complete_step(
    ledger: &mut TaskLedger,
    step_id: TaskStepId,
    spec: u8,
    run_id: AgentRunId,
    at: u64,
    direct_evidence: u8,
    verification_evidence: u8,
) -> Result<(), Box<dyn Error>> {
    ledger.start_step(step_id, run_id, timestamp(at)?)?;
    ledger.begin_step_verification(
        step_id,
        run_id,
        Some(TaskStepResultSummary::try_from_string(format!(
            "completed step {spec}"
        ))?),
        vec![TaskEvidenceId::from_bytes([direct_evidence; 32])],
        timestamp(at + 1)?,
    )?;
    ledger.finish_step_verification(
        step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([spec; 32]),
            VerificationSpecId::from_bytes([spec; 32]),
            run_id,
            StepVerificationOutcome::Passed,
            vec![TaskEvidenceId::from_bytes([verification_evidence; 32])],
            timestamp(at + 2)?,
        )?,
    )?;
    Ok(())
}

fn fail_verification(
    ledger: &mut TaskLedger,
    step_id: TaskStepId,
    spec: u8,
    run_id: AgentRunId,
    at: u64,
    direct_evidence: u8,
    verification_evidence: u8,
) -> Result<(), Box<dyn Error>> {
    ledger.start_step(step_id, run_id, timestamp(at)?)?;
    ledger.begin_step_verification(
        step_id,
        run_id,
        Some(TaskStepResultSummary::try_from_string(
            "verification remains open".to_owned(),
        )?),
        vec![TaskEvidenceId::from_bytes([direct_evidence; 32])],
        timestamp(at + 1)?,
    )?;
    ledger.finish_step_verification(
        step_id,
        StepVerification::new(
            StepVerificationId::from_bytes([spec; 32]),
            VerificationSpecId::from_bytes([spec; 32]),
            run_id,
            StepVerificationOutcome::Failed {
                summary: VerificationFailureSummary::try_from_string(
                    "fixture verification failed".to_owned(),
                )?,
            },
            vec![TaskEvidenceId::from_bytes([verification_evidence; 32])],
            timestamp(at + 2)?,
        )?,
    )?;
    Ok(())
}

fn claims(
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
) -> Result<Vec<TaskLensClaim>, Box<dyn Error>> {
    let symbol = memory_symbol()?;
    let module_id = ModuleId::from_bytes([32; 32]);
    let fact = TaskLensClaim::new(
        index_run_id,
        snapshot_id,
        ModuleCardClaimId::from_bytes([33; 32]),
        module_id,
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::Symbol(symbol.id()),
        VerifiedClaimKind::Fact,
        VerifiedClaimStatus::Active,
        Confidence::certain(),
        vec![ResolvedModuleCardEvidence::Symbol {
            id: ModuleCardEvidenceId::for_symbol_v1(&symbol),
            symbol,
        }],
    )?;
    let active_hypothesis = hypothesis(
        index_run_id,
        snapshot_id,
        module_id,
        34,
        VerifiedClaimStatus::Active,
    )?;
    let stale = hypothesis(
        index_run_id,
        snapshot_id,
        module_id,
        35,
        VerifiedClaimStatus::Stale,
    )?;
    let prior_source_run = hypothesis(
        IndexRunId::from_bytes([99; 32]),
        snapshot_id,
        module_id,
        36,
        VerifiedClaimStatus::Active,
    )?;
    Ok(vec![prior_source_run, stale, fact, active_hypothesis])
}

fn published_index() -> Result<PublishedIndex, Box<dyn Error>> {
    let snapshot_id = SnapshotId::from_bytes([21; 32]);
    let symbol = memory_symbol()?;
    let revision = symbol.revision().clone();
    let symbol_id = symbol.id();
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![revision.clone()],
        vec![symbol],
        Vec::new(),
        Vec::new(),
    )?;
    let ranking = RankProjection::new(
        snapshot_id,
        RankingPolicyVersion::v1(),
        vec![SymbolRank::new(
            symbol_id,
            RankScore::try_from_sum(1_000)?,
            SymbolRankSignals {
                in_degree: 0,
                out_degree: 0,
                centrality: Centrality::from_basis_points(1_000)?,
                degree_contribution: 0,
                centrality_contribution: 1_000,
                entrypoint_contribution: 0,
                public_export_contribution: 0,
                manifest_contribution: 0,
                test_contribution: 0,
            },
        )],
    )?;
    let module_id = ModuleId::from_bytes([32; 32]);
    let modules = ModuleProjection::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![RepositoryModule::new(
            module_id,
            ModuleKind::PathBoundary,
            Some(ModuleRoot::Repository),
            Vec::new(),
            ModuleSymbolSet::new(vec![symbol_id], false)?,
            ModuleSymbolSet::empty(),
            ModuleSymbolSet::empty(),
        )?],
        vec![ModuleMembership::new(
            module_id,
            symbol_id,
            ModuleMembershipEvidence::path(revision),
        )],
        RepositoryCard::new(
            snapshot_id,
            ModulePolicyVersion::v1(),
            vec![module_id],
            vec![IndexLanguage::Rust],
            ModuleSymbolSet::empty(),
            1,
            1,
        )?,
    )?;
    let publication = IndexPublication::new(graph, ranking, Vec::new(), modules)?;
    let run = IndexRunRecord::new(
        IndexRunId::from_bytes([20; 32]),
        snapshot_id,
        RankingPolicyVersion::v1(),
        IndexRunSequence::new(1)?,
        IndexRunStatus::Published,
    );
    Ok(PublishedIndex::new(run, publication)?)
}

fn memory_symbol() -> Result<GraphSymbol, Box<dyn Error>> {
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"src/memory.rs".to_vec())?,
        ContentHash::from_bytes([30; 32]),
    );
    let range = SourceRange::new(0, 16, SourcePosition::new(0, 0), SourcePosition::new(1, 0))?;
    Ok(GraphSymbol::new(
        SymbolId::from_bytes([31; 32]),
        revision,
        ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Function,
            SymbolName::try_from_string("compact".to_owned())?,
            range,
            range,
        )?,
    ))
}

fn hypothesis(
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    module_id: ModuleId,
    id: u8,
    status: VerifiedClaimStatus,
) -> Result<TaskLensClaim, Box<dyn Error>> {
    Ok(TaskLensClaim::new(
        index_run_id,
        snapshot_id,
        ModuleCardClaimId::from_bytes([id; 32]),
        module_id,
        ModuleClaimPolarity::Affirms,
        ModuleClaimPredicate::ArchitecturalIntent(ModuleClaimStatement::try_from_string(format!(
            "open hypothesis {id}"
        ))?),
        VerifiedClaimKind::Hypothesis,
        status,
        Confidence::from_basis_points(5_000)?,
        Vec::new(),
    )?)
}

fn timestamp(value: u64) -> Result<TaskLedgerTimestamp, Box<dyn Error>> {
    Ok(TaskLedgerTimestamp::from_unix_millis(value)?)
}
