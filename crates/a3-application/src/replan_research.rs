//! Read-only replan investigation inside the existing run budget and journal.
use crate::{AgentSourcePage, ResearchEvidenceAnchorId, ResearchEvidenceWindow};
use a3_domain::{
    AgentAction, AskResearchSourceId, ContentHash, ResearchAccessKind, ResearchAccessOutcome,
    ResearchQuestionDraft, ResearchQuestionId, ResearchQuestionKind, ResearchQuestionPriority,
    ResearchWorkError, ResearchWorkState, SnapshotId, TaskReplanReason, TaskStepId,
};

/// Durable step/snapshot ownership of the shared research aggregate. No source text is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplanResearchCheckpoint {
    /// Exact replacement step; never an implementation verification.
    pub step_id: TaskStepId,
    /// Immutable investigation snapshot.
    pub snapshot_id: SnapshotId,
    /// Same invariant-bearing aggregate used by Ask and Plan.
    pub work: ResearchWorkState,
}

impl ReplanResearchCheckpoint {
    /// Core creates the fixed obligation from the actual failure and intended outcome.
    pub fn new(
        step_id: TaskStepId,
        snapshot_id: SnapshotId,
        reason: &TaskReplanReason,
        intended_outcome: &str,
    ) -> Result<Self, ResearchWorkError> {
        let objective = format!(
            "Replan cause: {}\nIntended outcome: {intended_outcome}",
            reason.as_str()
        );
        let mut end = objective.len().min(2048);
        while !objective.is_char_boundary(end) {
            end -= 1;
        }
        let work = ResearchWorkState::new(objective.clone(), vec![ResearchQuestionDraft {
            request_fragment: objective[..end].to_owned(),
            outcome: "Locate the actual replan cause in current original code and explain the evidence-backed correction and remaining uncertainty. Reading alone does not resolve the cause or verify implementation.".to_owned(),
            priority: ResearchQuestionPriority::Required,
            kind: ResearchQuestionKind::Repository,
            dependencies: vec![],
        }])?;
        Ok(Self {
            step_id,
            snapshot_id,
            work,
        })
    }

    /// The existing four-read boundary survives process restarts; no budget is renewed.
    #[must_use]
    pub fn reads(&self) -> u16 {
        self.work
            .accesses()
            .iter()
            .fold(0u16, |n, a| n.saturating_add(a.starts))
    }

    /// A completed or failed identical read is not a new investigation step.
    #[must_use]
    pub fn permits(&self, action: &AgentAction) -> bool {
        self.reads() < 4
            && matches!(action, AgentAction::Search(_) | AgentAction::Inspect(_))
            && !self
                .work
                .accesses()
                .iter()
                .any(|a| a.key == read_key(action))
    }

    /// Records the actually attempted read, not the model's description of progress.
    pub fn record_read(
        &mut self,
        action: &AgentAction,
        succeeded: bool,
    ) -> Result<(), ResearchWorkError> {
        let key = read_key(action);
        let kind = if matches!(action, AgentAction::Search(_)) {
            ResearchAccessKind::IndexSearch
        } else {
            ResearchAccessKind::Inspect
        };
        let scope = ContentHash::from_bytes(*self.snapshot_id.as_bytes());
        self.work
            .begin_access(ResearchQuestionId::FIRST, scope, key, kind)?;
        self.work.finish_access(
            ResearchQuestionId::FIRST,
            scope,
            key,
            if succeeded {
                ResearchAccessOutcome::Completed
            } else {
                ResearchAccessOutcome::Unavailable
            },
        )?;
        Ok(())
    }
}

fn read_key(action: &AgentAction) -> ContentHash {
    let mut hash = blake3::Hasher::new_derive_key("a3.replan-read.v1");
    match action {
        AgentAction::Search(search) => {
            hash.update(b"search\0");
            hash.update(search.query().as_str().trim().as_bytes());
            hash.update(&search.limit().get().to_le_bytes());
        }
        AgentAction::Inspect(inspect) => match inspect.target() {
            a3_domain::AgentInspectTarget::File(file) => {
                hash.update(b"file\0");
                hash.update(file.path().as_bytes());
                hash.update(&file.start_line().get().to_le_bytes());
                hash.update(&file.line_count().get().to_le_bytes());
            }
            a3_domain::AgentInspectTarget::Test(selector) => {
                hash.update(b"test\0");
                hash.update(selector.as_str().as_bytes());
            }
            target => {
                hash.update(format!("{target:?}").as_bytes());
            }
        },
        _ => {
            hash.update(b"non-read");
        }
    }
    ContentHash::from_bytes(*hash.finalize().as_bytes())
}

/// Volatile, bounded originals accompany the durable state only during this attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplanResearchContext {
    /// Durable owner and obligation.
    pub checkpoint: ReplanResearchCheckpoint,
    /// Actual Safe Reader pages, not parsed search previews or remembered summaries.
    pub pages: Vec<AgentSourcePage>,
}

impl ReplanResearchContext {
    /// Canonical exact-window receipt, independent of turn or presentation IDs.
    #[must_use]
    pub fn packet(&self) -> ContentHash {
        let mut keys = self
            .pages
            .iter()
            .map(|p| (p.revision(), p.range()))
            .collect::<Vec<_>>();
        keys.sort_by(|a, b| {
            a.0.path()
                .cmp(b.0.path())
                .then(
                    a.0.content_hash()
                        .as_bytes()
                        .cmp(b.0.content_hash().as_bytes()),
                )
                .then(a.1.start_byte().cmp(&b.1.start_byte()))
                .then(a.1.end_byte().cmp(&b.1.end_byte()))
        });
        keys.dedup();
        let mut hash = blake3::Hasher::new_derive_key("a3.replan-original-packet.v1");
        for (revision, range) in keys {
            hash.update(revision.path().as_bytes());
            hash.update(revision.content_hash().as_bytes());
            hash.update(&range.start_byte().to_le_bytes());
            hash.update(&range.end_byte().to_le_bytes());
        }
        ContentHash::from_bytes(*hash.finalize().as_bytes())
    }

    /// Only a novel nonempty original packet starts a V5 analysis, not another read decision.
    #[must_use]
    pub fn should_analyze(&self) -> bool {
        !self.checkpoint.work.ready_to_finish()
            && !self.pages.is_empty()
            && self.pages.len() <= 8
            && self
                .checkpoint
                .work
                .question(ResearchQuestionId::FIRST)
                .is_some_and(|q| !q.attempts().contains(&self.packet()))
    }

    /// Admission windows match exactly the E-labeled original pages in the compiled context.
    pub fn windows(
        &self,
    ) -> Result<Vec<ResearchEvidenceWindow<'_>>, crate::AskResearchDecisionDecodeError> {
        self.pages
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ordinal = u16::try_from(i + 1)
                    .map_err(|_| crate::AskResearchDecisionDecodeError::InvalidValue)?;
                let mut hash = blake3::Hasher::new_derive_key("a3.replan-source.v1");
                hash.update(p.revision().path().as_bytes());
                hash.update(p.revision().content_hash().as_bytes());
                Ok(ResearchEvidenceWindow {
                    anchor: Some(ResearchEvidenceAnchorId::new(ordinal)?),
                    ordinal,
                    source_id: AskResearchSourceId::from_bytes(*hash.finalize().as_bytes()),
                    revision: p.revision(),
                    range: p.range(),
                    text: p.text(),
                })
            })
            .collect()
    }

    /// Bounded public contract and exact sources. The compiler accounts for every byte.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = format!(
            "[REPLAN_RESEARCH] interpretation only; not implementation verification\n{}\n",
            self.checkpoint.work.objective()
        );
        for q in self.checkpoint.work.questions() {
            text.push_str(&format!(
                "Q{} {:?}: {}\n",
                q.id().get(),
                q.status(),
                q.definition().outcome
            ));
            if let Some(result) = q.result() {
                text.push_str(result.text());
                text.push('\n');
            }
        }
        if self.should_analyze() {
            for (i, p) in self.pages.iter().enumerate() {
                text.push_str(&format!(
                    "\nE{} original path={} hash={:?} bytes={}..{}\n{}\n",
                    i + 1,
                    String::from_utf8_lossy(p.revision().path().as_bytes()),
                    p.revision().content_hash(),
                    p.range().start_byte(),
                    p.range().end_byte(),
                    p.text()
                ));
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replan_reads_use_values_not_redacted_debug_lengths_and_keep_the_outer_limit()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut checkpoint = ReplanResearchCheckpoint::new(
            TaskStepId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            &TaskReplanReason::try_from_string("missing source".to_owned())?,
            "correct serialization",
        )?;
        for query in ["alpha", "bravo", "delta", "gamma"] {
            let action = AgentAction::Search(a3_domain::AgentSearchAction::new(
                a3_domain::AgentSearchQuery::try_from_string(query.to_owned())?,
                a3_domain::AgentSearchLimit::new(5)?,
            ));
            assert!(
                checkpoint.permits(&action),
                "equal-length targets are not duplicates"
            );
            checkpoint.record_read(&action, true)?;
            assert!(!checkpoint.permits(&action));
        }
        assert_eq!(checkpoint.clone().reads(), 4);
        assert!(
            !checkpoint.work.ready_to_finish(),
            "four reads cannot answer a question"
        );
        Ok(())
    }
}
