//! Volatile, content-free identity of the exact original supplied in a Plan request.

use super::{PlanPhaseFailure, PreparedPlanPhase};
use crate::{AgentSourcePage, AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader};
use a3_domain::{
    AgentFileInspection, AgentFileLineCount, ContentHash, FileRevision, ProjectIdentity,
    SourcePosition, SourceRange,
};

pub(super) struct PlanSourceBinding {
    revision: FileRevision,
    range: SourceRange,
    inspection: AgentFileInspection,
    text_digest: ContentHash,
}

impl PlanSourceBinding {
    pub(super) fn new(page: &AgentSourcePage) -> Result<Self, PlanPhaseFailure> {
        let range = page.range();
        if range.is_empty()
            || range.start_position() != SourcePosition::new(page.start_line().get() - 1, 0)
            || u32::try_from(page.text().len()).ok() != Some(range.len())
        {
            return Err(PlanPhaseFailure::InvalidOffer);
        }
        // Reader pages contain complete lines. LF and CRLF both count as one ending.
        let count =
            u16::try_from(page.text().lines().count()).map_err(|_| PlanPhaseFailure::Capacity)?;
        let count = AgentFileLineCount::new(count).map_err(|_| PlanPhaseFailure::Capacity)?;
        Ok(Self {
            revision: page.revision().clone(),
            range,
            inspection: AgentFileInspection::new(
                page.revision().path().clone(),
                page.start_line(),
                count,
            ),
            text_digest: text_digest(page),
        })
    }
}

impl PreparedPlanPhase {
    /// Re-reads exactly the originally supplied page through the existing secure Reader.
    pub(crate) async fn revalidate_source(
        &self,
        project: &ProjectIdentity,
        reader: &dyn AgentSourceReader,
        control: &dyn AgentSourceReadControl,
    ) -> Result<(), AgentSourceReadFailure> {
        let Some(bound) = self.source.as_ref() else {
            return Ok(());
        };
        let current = reader
            .read_page(project, &bound.revision, &bound.inspection, control)
            .await?;
        if current.revision() != &bound.revision
            || current.range() != bound.range
            || current.start_line() != bound.inspection.start_line()
            || text_digest(&current) != bound.text_digest
        {
            return Err(AgentSourceReadFailure::Stale);
        }
        Ok(())
    }
}

fn text_digest(page: &AgentSourcePage) -> ContentHash {
    ContentHash::from_bytes(*blake3::hash(page.text().as_bytes()).as_bytes())
}
