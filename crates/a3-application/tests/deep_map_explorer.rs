//! Contract tests for the bounded read-only Deep Map explorer state machine.

use a3_application::{
    DeepMapExplorerFailure, DeepMapExplorerStatus, DeepMapReadControl, DeepMapReadFailure,
    DeepMapReadFuture, DeepMapReadTimeout, DeepMapReadTools, ExploreDeepMap, ExplorerModelControl,
    ExplorerModelFailure, ExplorerModelFuture, ExplorerModelProvider, ExplorerModelRequest,
    ExplorerModelRequestPhase, ExplorerModelTimeout, ExplorerObservation, RawExplorerModelOutput,
};
use a3_domain::{
    CanonicalDirectory, Centrality, Confidence, ContentHash, DeepMapPlanner, ExploreBudget,
    ExplorePlan, ExploreTarget, ExplorerCheckpoint, ExplorerSearchAction, FileRevision, GitHead,
    GitReferenceName, GraphSymbol, IndexLanguage, IndexPublication, IndexRunId, IndexRunRecord,
    IndexRunSequence, IndexRunStatus, LinkedGraph, LocalSymbolId, MapperProfileVersion,
    ModuleCardEvidenceId, ModuleCardField, ModuleCardId, ModuleCardProposal,
    ModuleCardProposalEnvelope, ModuleCardSchemaVersion, ModuleCoverageSnapshot, ModuleId,
    ModuleKind, ModuleMembership, ModuleMembershipEvidence, ModulePolicyVersion, ModuleProjection,
    ModuleRoot, ModuleSymbolSet, ParsedSymbol, ProjectIdentity, ProposedModuleCardField,
    RankProjection, RankScore, RankingPolicyVersion, RepositoryCard, RepositoryId,
    RepositoryIdentity, RepositoryModule, RepositoryPath, SnapshotId, SourcePosition, SourceRange,
    SymbolId, SymbolKind, SymbolName, SymbolRank, SymbolRankSignals, WorktreeAnchorId, WorktreeId,
    WorktreeIdentity,
};
use futures::executor::block_on;
use serde_json::json;
use std::error::Error;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const OBSERVED_EVIDENCE: [u8; 32] = [9; 32];
type ProviderCall = (u16, ExplorerModelRequestPhase, bool);

#[derive(Debug, Clone, Copy)]
enum ProviderBehavior {
    Valid,
    InvalidOnce,
    AlwaysInvalid,
    WrongCardId,
}

#[derive(Debug)]
struct ScriptedProvider {
    behavior: ProviderBehavior,
    calls: Mutex<Vec<ProviderCall>>,
    call_count: AtomicUsize,
}

impl ScriptedProvider {
    fn new(behavior: ProviderBehavior) -> Self {
        Self {
            behavior,
            calls: Mutex::new(Vec::new()),
            call_count: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> Result<Vec<ProviderCall>, Box<dyn Error>> {
        Ok(self
            .calls
            .lock()
            .map_err(|_| "provider call log was poisoned")?
            .clone())
    }
}

impl ExplorerModelProvider for ScriptedProvider {
    fn complete<'a>(
        &'a self,
        request: &'a ExplorerModelRequest,
        _timeout: ExplorerModelTimeout,
        control: &'a dyn ExplorerModelControl,
    ) -> ExplorerModelFuture<'a> {
        if control.is_cancelled() {
            return Box::pin(async { Err(ExplorerModelFailure::Cancelled) });
        }
        let has_observation = request.observation().is_some();
        let logged = self.calls.lock().map(|mut calls| {
            calls.push((request.step_sequence(), request.phase(), has_observation));
        });
        if logged.is_err() {
            return Box::pin(async { Err(ExplorerModelFailure::Rejected) });
        }
        let call_index = self.call_count.fetch_add(1, Ordering::SeqCst);
        let invalid = matches!(self.behavior, ProviderBehavior::AlwaysInvalid)
            || matches!(self.behavior, ProviderBehavior::InvalidOnce) && call_index == 0;
        if invalid {
            return Box::pin(async {
                RawExplorerModelOutput::new("not json".to_owned())
                    .map_err(|_| ExplorerModelFailure::InvalidResponse)
            });
        }

        let raw = if let Some(observation) = request.observation() {
            let evidence = hex(observation.evidence_ids()[0].as_bytes());
            let fields = request
                .expected_fields()
                .iter()
                .map(|field| {
                    json!({
                        "field": field_name(*field),
                        "values": [format!("verified preview for {}", field_name(*field))],
                        "evidence_ids": [evidence]
                    })
                })
                .collect::<Vec<_>>();
            let module_id = request.module_id().to_string();
            let card_id = if matches!(self.behavior, ProviderBehavior::WrongCardId) {
                "00".repeat(32)
            } else {
                hex(ModuleCardId::for_module_fields_v1(
                    request.module_id(),
                    request.expected_fields(),
                )
                .as_bytes())
            };
            let proposal = json!({
                "schema_version": 1,
                "action": {
                    "kind": "propose",
                    "proposal": {
                        "card_id": card_id,
                        "module_id": module_id,
                        "snapshot_id": request.snapshot_id().to_string(),
                        "schema_version": 1,
                        "mapper_profile_version": 1,
                        "confidence_basis_points": Confidence::certain().basis_points(),
                        "fields": fields
                    }
                }
            });
            proposal.to_string()
        } else {
            json!({
                "schema_version": 1,
                "action": {
                    "kind": "inspect",
                    "expected_gain_basis_points": 100,
                    "gain_rationale": "inspect the exact current plan target"
                }
            })
            .to_string()
        };
        Box::pin(async move {
            RawExplorerModelOutput::new(raw).map_err(|_| ExplorerModelFailure::InvalidResponse)
        })
    }
}

#[derive(Debug, Default)]
struct RecordingReadTools {
    calls: AtomicUsize,
}

impl DeepMapReadTools for RecordingReadTools {
    fn inspect<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _snapshot_id: SnapshotId,
        _target: &'a ExploreTarget,
        _timeout: DeepMapReadTimeout,
        control: &'a dyn DeepMapReadControl,
    ) -> DeepMapReadFuture<'a> {
        self.read(control)
    }

    fn search<'a>(
        &'a self,
        _project: &'a ProjectIdentity,
        _snapshot_id: SnapshotId,
        _action: &'a ExplorerSearchAction,
        _timeout: DeepMapReadTimeout,
        control: &'a dyn DeepMapReadControl,
    ) -> DeepMapReadFuture<'a> {
        self.read(control)
    }
}

impl RecordingReadTools {
    fn read<'a>(&'a self, control: &'a dyn DeepMapReadControl) -> DeepMapReadFuture<'a> {
        if control.is_cancelled() {
            return Box::pin(async { Err(DeepMapReadFailure::Cancelled) });
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            ExplorerObservation::found(
                "bounded normalized published-index evidence".to_owned(),
                vec![ModuleCardEvidenceId::from_bytes(OBSERVED_EVIDENCE)],
                false,
            )
            .map_err(|_| DeepMapReadFailure::InvalidResponse)
        })
    }
}

#[derive(Debug)]
struct TestControl {
    cancelled: Arc<AtomicBool>,
}

impl DeepMapReadControl for TestControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl ExplorerModelControl for TestControl {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn cancelled(&self) -> a3_application::ModelCancellationFuture<'_> {
        if ExplorerModelControl::is_cancelled(self) {
            Box::pin(futures::future::ready(()))
        } else {
            Box::pin(futures::future::pending())
        }
    }
}

#[test]
fn resume_checkpoint_does_not_repeat_confirmed_steps() -> Result<(), Box<dyn Error>> {
    let plan = plan_fixture()?;
    let project = project_fixture()?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let control = TestControl {
        cancelled: Arc::clone(&cancelled),
    };
    let provider = ScriptedProvider::new(ProviderBehavior::Valid);
    let tools = RecordingReadTools::default();
    let explorer = ExploreDeepMap::version_one(&provider, &tools);
    let mut checkpoint = ExplorerCheckpoint::new(&plan);
    checkpoint.confirm_next(&plan, proposal_for_step(&plan, 0)?)?;

    let resumed = block_on(explorer.execute(&project, &plan, checkpoint, &control))?;
    assert_eq!(resumed.status(), DeepMapExplorerStatus::Completed);
    assert_eq!(
        resumed.checkpoint().confirmed_step_count(),
        plan.steps().len()
    );
    assert_eq!(tools.calls.load(Ordering::SeqCst), plan.steps().len() - 1);
    assert!(
        provider
            .calls()?
            .iter()
            .all(|(sequence, _, _)| *sequence >= 2)
    );
    Ok(())
}

#[test]
fn cancellation_crosses_neither_provider_nor_read_boundary() -> Result<(), Box<dyn Error>> {
    let plan = plan_fixture()?;
    let project = project_fixture()?;
    let cancelled = Arc::new(AtomicBool::new(true));
    let control = TestControl {
        cancelled: Arc::clone(&cancelled),
    };
    let provider = ScriptedProvider::new(ProviderBehavior::Valid);
    let tools = RecordingReadTools::default();
    let outcome = block_on(ExploreDeepMap::version_one(&provider, &tools).execute(
        &project,
        &plan,
        ExplorerCheckpoint::new(&plan),
        &control,
    ))?;

    assert_eq!(outcome.status(), DeepMapExplorerStatus::Cancelled);
    assert_eq!(outcome.checkpoint().confirmed_step_count(), 0);
    assert!(provider.calls()?.is_empty());
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn one_invalid_output_gets_exactly_one_repair_and_then_completes() -> Result<(), Box<dyn Error>> {
    let plan = plan_fixture()?;
    let project = project_fixture()?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let control = TestControl {
        cancelled: Arc::clone(&cancelled),
    };
    let provider = ScriptedProvider::new(ProviderBehavior::InvalidOnce);
    let tools = RecordingReadTools::default();
    let outcome = block_on(ExploreDeepMap::version_one(&provider, &tools).execute(
        &project,
        &plan,
        ExplorerCheckpoint::new(&plan),
        &control,
    ))?;

    assert_eq!(outcome.status(), DeepMapExplorerStatus::Completed);
    assert_eq!(
        provider
            .calls()?
            .iter()
            .filter(|(_, phase, _)| matches!(phase, ExplorerModelRequestPhase::Repair(_)))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn invalid_original_and_repair_are_never_executed_as_tools() -> Result<(), Box<dyn Error>> {
    let plan = plan_fixture()?;
    let project = project_fixture()?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let control = TestControl {
        cancelled: Arc::clone(&cancelled),
    };
    let provider = ScriptedProvider::new(ProviderBehavior::AlwaysInvalid);
    let tools = RecordingReadTools::default();
    let result = block_on(ExploreDeepMap::version_one(&provider, &tools).execute(
        &project,
        &plan,
        ExplorerCheckpoint::new(&plan),
        &control,
    ));

    assert_eq!(result, Err(DeepMapExplorerFailure::InvalidModelOutput));
    assert_eq!(provider.calls()?.len(), 2);
    assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn model_generated_card_identity_is_rejected_after_one_repair() -> Result<(), Box<dyn Error>> {
    let plan = plan_fixture()?;
    let project = project_fixture()?;
    let control = TestControl {
        cancelled: Arc::new(AtomicBool::new(false)),
    };
    let provider = ScriptedProvider::new(ProviderBehavior::WrongCardId);
    let tools = RecordingReadTools::default();
    let result = block_on(ExploreDeepMap::version_one(&provider, &tools).execute(
        &project,
        &plan,
        ExplorerCheckpoint::new(&plan),
        &control,
    ));

    assert_eq!(result, Err(DeepMapExplorerFailure::InvalidModelOutput));
    assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider.calls()?.len(), 3);
    Ok(())
}

#[test]
fn valid_run_uses_exactly_one_read_per_planned_step() -> Result<(), Box<dyn Error>> {
    let plan = plan_fixture()?;
    let project = project_fixture()?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let control = TestControl {
        cancelled: Arc::clone(&cancelled),
    };
    let provider = ScriptedProvider::new(ProviderBehavior::Valid);
    let tools = RecordingReadTools::default();
    let outcome = block_on(ExploreDeepMap::version_one(&provider, &tools).execute(
        &project,
        &plan,
        ExplorerCheckpoint::new(&plan),
        &control,
    ))?;

    assert_eq!(outcome.status(), DeepMapExplorerStatus::Completed);
    assert_eq!(tools.calls.load(Ordering::SeqCst), plan.steps().len());
    assert!(outcome.checkpoint().is_complete_for(&plan));
    Ok(())
}

#[test]
fn provider_request_debug_redacts_manifest_paths_and_observation_content()
-> Result<(), Box<dyn Error>> {
    let plan = plan_fixture()?;
    let observation = ExplorerObservation::found(
        "sensitive source preview".to_owned(),
        vec![ModuleCardEvidenceId::from_bytes(OBSERVED_EVIDENCE)],
        false,
    )?;
    let request = ExplorerModelRequest::for_step(
        &plan,
        &plan.steps()[0],
        Some(observation),
        ExplorerModelRequestPhase::Primary,
    );
    let debug = format!("{request:?}");
    assert!(!debug.contains("Cargo.toml"));
    assert!(!debug.contains("sensitive source preview"));
    Ok(())
}

fn proposal_for_step(
    plan: &ExplorePlan,
    step_index: usize,
) -> Result<ModuleCardProposal, Box<dyn Error>> {
    let step = plan
        .steps()
        .get(step_index)
        .ok_or("fixture step is unavailable")?;
    let evidence = ModuleCardEvidenceId::from_bytes(OBSERVED_EVIDENCE);
    let fields = step
        .coverage_fields()
        .iter()
        .map(|field| {
            ProposedModuleCardField::new(
                *field,
                vec![format!("confirmed {}", field_name(*field))],
                vec![evidence],
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ModuleCardProposal::new(
        ModuleCardProposalEnvelope::new(
            ModuleCardId::for_module_fields_v1(step.module_id(), step.coverage_fields()),
            step.module_id(),
            plan.snapshot_id(),
            ModuleCardSchemaVersion::V1,
            MapperProfileVersion::V1,
            Confidence::certain(),
        ),
        fields,
        512,
    )?)
}

fn field_name(field: ModuleCardField) -> &'static str {
    match field {
        ModuleCardField::Title => "title",
        ModuleCardField::Paths => "paths",
        ModuleCardField::Purpose => "purpose",
        ModuleCardField::Responsibilities => "responsibilities",
        ModuleCardField::PublicSurface => "public_surface",
        ModuleCardField::Entrypoints => "entrypoints",
        ModuleCardField::Dependencies => "dependencies",
        ModuleCardField::DataFlows => "data_flows",
        ModuleCardField::Invariants => "invariants",
        ModuleCardField::Tests => "tests",
        ModuleCardField::Risks => "risks",
        ModuleCardField::OpenQuestions => "open_questions",
    }
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn project_fixture() -> Result<ProjectIdentity, Box<dyn Error>> {
    let repository_id = RepositoryId::from_bytes([21; 32]);
    let path = std::env::current_dir()?;
    let repository = RepositoryIdentity::new(
        repository_id,
        CanonicalDirectory::from_canonicalized(path.clone())?,
        None,
    );
    let worktree = WorktreeIdentity::new(
        WorktreeId::from_bytes([22; 32]),
        WorktreeAnchorId::from_bytes([23; 32]),
        repository_id,
        CanonicalDirectory::from_canonicalized(path)?,
    );
    let head = GitHead::Unborn {
        reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
    };
    Ok(ProjectIdentity::new(repository, worktree, head)?)
}

fn plan_fixture() -> Result<ExplorePlan, Box<dyn Error>> {
    let published = published_fixture()?;
    let coverage = ModuleCoverageSnapshot::empty(
        published.run().snapshot_id(),
        a3_domain::ModuleCardSchemaVersion::V1,
    );
    Ok(DeepMapPlanner::v1().plan(&published, &coverage, ExploreBudget::DEFAULT)?)
}

fn published_fixture() -> Result<a3_domain::PublishedIndex, Box<dyn Error>> {
    let snapshot_id = SnapshotId::from_bytes([1; 32]);
    let manifest = revision("Cargo.toml", 2)?;
    let source = revision("src/lib.rs", 3)?;
    let symbol_id = SymbolId::from_bytes([4; 32]);
    let range = SourceRange::new(0, 1, SourcePosition::new(0, 0), SourcePosition::new(0, 1))?;
    let symbol = GraphSymbol::new(
        symbol_id,
        source.clone(),
        ParsedSymbol::new(
            LocalSymbolId::new(1)?,
            SymbolKind::Function,
            SymbolName::try_from_string("main".to_owned())?,
            range,
            range,
        )?,
    );
    let graph = LinkedGraph::new(
        snapshot_id,
        vec![manifest.clone(), source.clone()],
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
    let module_id = ModuleId::from_bytes([5; 32]);
    let featured = ModuleSymbolSet::new(vec![symbol_id], false)?;
    let module = RepositoryModule::new(
        module_id,
        ModuleKind::ManifestBoundary,
        Some(ModuleRoot::Repository),
        vec![manifest.clone()],
        featured.clone(),
        featured.clone(),
        ModuleSymbolSet::empty(),
    )?;
    let membership = ModuleMembership::new(
        module_id,
        symbol_id,
        ModuleMembershipEvidence::manifest(source, manifest.clone()),
    );
    let card = RepositoryCard::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![module_id],
        vec![IndexLanguage::Rust],
        featured,
        2,
        1,
    )?;
    let modules = ModuleProjection::new(
        snapshot_id,
        ModulePolicyVersion::v1(),
        vec![module],
        vec![membership],
        card,
    )?;
    let publication = IndexPublication::new(graph, ranking, vec![manifest], modules)?;
    let run = IndexRunRecord::new(
        IndexRunId::from_bytes([6; 32]),
        snapshot_id,
        RankingPolicyVersion::v1(),
        IndexRunSequence::new(1)?,
        IndexRunStatus::Published,
    );
    Ok(a3_domain::PublishedIndex::new(run, publication)?)
}

fn revision(path: &str, hash: u8) -> Result<FileRevision, Box<dyn Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
        ContentHash::from_bytes([hash; 32]),
    ))
}
