//! One non-recursive, source-bound supplement before the first repository analysis.
use super::*;
use a3_domain::{GraphEndpoint, SyntaxRelationKind};

const MAX_EDGES: usize = 4096;
const MAX_FILES: usize = 4;

pub(super) fn prepare(
    controller: &mut BoundedResearchController,
    actions: Vec<AskResearchAction>,
    elapsed_millis: u64,
) -> Result<Option<a3_application::ResearchActionBatch>, AgentSessionManagerFailure> {
    match controller.prepare_work_frontier(actions, elapsed_millis) {
        Ok(batch) => Ok(batch),
        // Revalidation may have consumed the final time slice. Let the normal
        // controller produce TimeLimit, not an invented invalid-model-output error.
        Err(a3_application::ResearchControllerError::TimedOut) => Ok(None),
        Err(_) => Err(AgentSessionManagerFailure::InvalidOutput),
    }
}

pub(super) fn allowed(
    state: &AskResearchWorkingSet,
    controller: &BoundedResearchController,
    phase: a3_application::ResearchOutputPhase,
    packet_bytes: usize,
    elapsed_millis: u64,
) -> bool {
    state.evidence_limit.saturating_sub(packet_bytes) >= 1024
        && controller.decisions_used().saturating_add(1) < controller.limits().model_decisions()
        && controller.actions_used() < controller.limits().read_actions()
        && elapsed_millis < controller.limits().duration_millis()
        && matches!(
            phase,
            a3_application::ResearchOutputPhase::Analyze(_)
                | a3_application::ResearchOutputPhase::SummarizeOriginals(_)
        )
        && state.complete_required_originals_delivered()
}

pub(super) fn candidates(
    published: &a3_domain::PublishedIndex,
    state: &AskResearchWorkingSet,
) -> Vec<AskResearchAction> {
    let graph = published.publication().graph();
    let mut revisions = Vec::new();
    for edge in graph.edges().iter().take(MAX_EDGES) {
        if edge.kind() != SyntaxRelationKind::Calls
            || edge.snapshot_id() != graph.snapshot_id()
            || !state
                .work_required_revisions
                .contains(edge.evidence().revision())
            || !state.current_delivery.iter().any(|window| {
                window.revision == *edge.evidence().revision()
                    && window.start <= edge.evidence().range().start_position()
                    && window.end >= edge.evidence().range().end_position()
            })
        {
            continue;
        }
        let GraphEndpoint::Symbol(id) = edge.target() else {
            continue;
        };
        let Ok(index) = graph
            .symbols()
            .binary_search_by_key(id, |symbol| symbol.id())
        else {
            continue;
        };
        let revision = graph.symbols()[index].revision();
        if !state.work_required_revisions.contains(revision)
            && !state.complete_files.contains(revision)
            && !revisions.contains(revision)
        {
            revisions.push(revision.clone());
            if revisions.len() == MAX_FILES {
                break;
            }
        }
    }
    state.novel_work_accesses(
        published,
        revisions
            .into_iter()
            .map(|revision| AskResearchAction::InspectPath {
                path: model_safe_path(revision.path()),
                start_line: 1,
            })
            .collect(),
    )
}

impl AgentAskResearcher {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn supplement_direct_sources(
        &self,
        project: &ProjectIdentity,
        published: &Arc<a3_domain::PublishedIndex>,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        controller: &mut BoundedResearchController,
        started: Instant,
        query: &str,
        packet_bytes: usize,
        control: &JobContext,
    ) -> Result<bool, AgentSessionManagerFailure> {
        // Do not spend optional reads in a final-only, design, repair or exhausted phase.
        if !allowed(
            state,
            controller,
            research_work::WorkGuard::new(query, state).output_phase(),
            packet_bytes,
            elapsed_millis(started),
        ) {
            return Ok(false);
        }
        let actions = candidates(published, state);
        if actions.is_empty() {
            return Ok(false);
        }
        state.evidence_guard(project).validate(control).await?;
        let Some(batch) = prepare(controller, actions, elapsed_millis(started))? else {
            return Ok(false);
        };
        let before = state.evidence_revision;
        state.event_sequence = state.event_sequence.saturating_add(1);
        self.append_running_event(
            project, turn, state.event_sequence, AskResearchPhase::SelectingEvidence,
            "Core ergänzt Originalquellen direkt verknüpfter Aufrufe innerhalb des verbleibenden Recherchebudgets",
            None, AskResearchCompleteness::Limited,
        ).await?;
        // Optional enrichment must not replace an explicitly selected source region.
        let focus = state.focus.clone();
        self.execute_actions(
            project,
            published,
            turn,
            state,
            batch.actions().to_vec(),
            control,
        )
        .await?;
        state.focus = focus;
        controller.finish_round(before, state.evidence_revision);
        Ok(state.evidence_revision != before)
    }
}
