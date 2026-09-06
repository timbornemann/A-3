//! Real Safe Reader hydration: restart restores bytes, never read or analysis budgets.
use super::*;
use a3_application::{
    AgentSourceReadControl, AgentSourceReader, ReplanResearchCheckpoint, ReplanResearchContext,
};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, AgentFileStartLine, AgentInspectAction,
    AgentInspectTarget, ContentHash, FileRevision, RepositoryPath, ResearchQuestionId, SnapshotId,
};

use crate::index_test_support as support;

#[derive(Debug)]
struct Active;
impl AgentSourceReadControl for Active {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[test]
fn replan_hydration_restores_exact_originals_without_reopening_receipts_or_allowing_stale_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = support::TempDirectory::new()?;
    directory.git(["init", "--initial-branch=main"])?;
    let body = "def save():\n    return 'ä'\n";
    directory.write("module.py", body)?;
    let project = a3_workspace::RepositoryInspector::new().inspect(directory.path())?;
    let revision = FileRevision::new(
        RepositoryPath::try_from_bytes(b"module.py".to_vec())?,
        ContentHash::from_bytes(*blake3::hash(body.as_bytes()).as_bytes()),
    );
    let request = AgentFileInspection::new(
        revision.path().clone(),
        AgentFileStartLine::new(1)?,
        AgentFileLineCount::new(2)?,
    );
    let page = futures::executor::block_on(
        WorkspaceAgentSourceReader.read_page(&project, &revision, &request, &Active),
    )?;
    let mut research = ReplanResearchContext {
        checkpoint: ReplanResearchCheckpoint::new(
            TaskStepId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            &TaskReplanReason::try_from_string("Investigate save".to_owned())?,
            "Preserve stored values",
        )?,
        pages: vec![page.clone()],
    };
    research.checkpoint.record_read(
        &AgentAction::Inspect(AgentInspectAction::new(AgentInspectTarget::File(request))),
        true,
    )?;
    assert!(research.should_analyze());
    let packet = research.packet();
    research
        .checkpoint
        .work
        .begin_analysis(ResearchQuestionId::FIRST, packet)?;
    let checkpoint = research.checkpoint.clone();
    research.pages.clear();
    futures::executor::block_on(hydrate_replan_originals(
        &WorkspaceAgentSourceReader,
        &project,
        &mut research,
        vec![page.evidence()],
        &Active,
    ))?;
    assert_eq!(research.pages, vec![page.clone()]);
    assert_eq!(research.packet(), packet);
    assert_eq!(research.checkpoint, checkpoint);
    assert_eq!(research.checkpoint.reads(), 1);
    assert!(
        !research.should_analyze(),
        "hydration cannot reanalyze the same acknowledged packet"
    );
    research.pages.clear();
    directory.write("module.py", "def save():\n    return 'changed'\n")?;
    assert!(
        futures::executor::block_on(hydrate_replan_originals(
            &WorkspaceAgentSourceReader,
            &project,
            &mut research,
            vec![page.evidence()],
            &Active
        ))
        .is_err()
    );
    assert!(research.pages.is_empty());
    assert_eq!(research.checkpoint, checkpoint);
    Ok(())
}
