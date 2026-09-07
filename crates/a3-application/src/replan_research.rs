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
    /// Unresolved navigation hint; never a result, tool argument or permission.
    pub pending_need: Option<crate::ReplanEvidenceNeed>,
}

/// Content-free reason a proposed localization read cannot become a new tool attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplanReadRejection {
    /// The existing durable four-read limit is exhausted.
    BudgetExhausted,
    /// The proposed action is outside the read-only localization subset.
    NotReadAction,
    /// The same canonical action value already has an access attempt.
    RepeatedRead,
    /// A retained V1 claim receipt lost its target identity through Debug redaction.
    AmbiguousLegacyClaim,
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
            pending_need: None,
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
        self.validate_read(action).is_ok()
    }

    /// Validates the unchanged read boundary without discarding the reason for rejection.
    pub fn validate_read(&self, action: &AgentAction) -> Result<(), ReplanReadRejection> {
        if self.reads() >= 4 {
            return Err(ReplanReadRejection::BudgetExhausted);
        }
        if !matches!(action, AgentAction::Search(_) | AgentAction::Inspect(_)) {
            return Err(ReplanReadRejection::NotReadAction);
        }
        if matches!(action, AgentAction::Inspect(inspect) if matches!(inspect.target(), a3_domain::AgentInspectTarget::Claim(_)))
            && self
                .work
                .accesses()
                .iter()
                .any(|a| a.key == legacy_claim_read_key_v1())
        {
            return Err(ReplanReadRejection::AmbiguousLegacyClaim);
        }
        if self
            .work
            .accesses()
            .iter()
            .any(|a| a.key == read_key(action))
        {
            return Err(ReplanReadRejection::RepeatedRead);
        }
        Ok(())
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
    if let AgentAction::Inspect(inspect) = action
        && let a3_domain::AgentInspectTarget::Claim(id) = inspect.target()
    {
        let mut hash = blake3::Hasher::new_derive_key("a3.replan-claim-read.v2");
        hash.update(id.as_bytes());
        return ContentHash::from_bytes(*hash.finalize().as_bytes());
    }
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

/// Frozen historical fingerprint, not a currently recoverable claim identity.
pub(crate) fn legacy_claim_read_key_v1() -> ContentHash {
    let mut hash = blake3::Hasher::new_derive_key("a3.replan-read.v1");
    hash.update(b"Claim(ModuleCardClaimId(redacted))");
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
        original_packet(self.pages.iter().map(|p| (p.revision(), p.range())))
    }

    /// A restored hint is unusable until its exact original packet has been revalidated.
    #[must_use]
    pub fn validates_pending_need(&self) -> bool {
        self.checkpoint.pending_need.as_ref().is_none_or(|need| {
            need.validates_work(&self.checkpoint.work)
                && need.validates_originals(self.checkpoint.work.objective(), &self.pages)
        })
    }

    /// Only a novel nonempty original packet starts analysis, not another read decision.
    #[must_use]
    pub fn should_analyze(&self) -> bool {
        !self.checkpoint.work.ready_to_finish()
            && !self.pages.is_empty()
            && self.pages.len() <= 8
            && self.validates_pending_need()
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
        if let Some(need) = &self.checkpoint.pending_need
            && self.validates_pending_need()
        {
            text.push_str("Pending Q1 evidence need (navigation candidates only; not facts, actions or approval):\n");
            for target in need.need().targets() {
                text.push_str(target);
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

pub(crate) fn original_packet<'a>(
    originals: impl Iterator<Item = (&'a a3_domain::FileRevision, a3_domain::SourceRange)>,
) -> ContentHash {
    let mut keys = originals.collect::<Vec<_>>();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replan_distinct_claims_do_not_share_a_redacted_debug_read_key()
    -> Result<(), Box<dyn std::error::Error>> {
        let claim = |id| {
            AgentAction::Inspect(a3_domain::AgentInspectAction::new(
                a3_domain::AgentInspectTarget::Claim(a3_domain::ModuleCardClaimId::from_bytes(
                    [id; 32],
                )),
            ))
        };
        assert_ne!(
            read_key(&claim(1)),
            read_key(&claim(2)),
            "different claim identities collided"
        );
        let mut checkpoint = ReplanResearchCheckpoint::new(
            TaskStepId::from_bytes([1; 32]),
            SnapshotId::from_bytes([2; 32]),
            &TaskReplanReason::try_from_string("find the relevant claim".to_owned())?,
            "locate supporting original",
        )?;
        for id in 1..=4 {
            let action = claim(id);
            assert!(checkpoint.permits(&action));
            checkpoint.record_read(&action, id != 2)?;
            assert!(!checkpoint.permits(&action));
        }
        assert_eq!(checkpoint.reads(), 4);
        assert_eq!(
            checkpoint.validate_read(&claim(5)),
            Err(ReplanReadRejection::BudgetExhausted)
        );
        assert!(!checkpoint.work.ready_to_finish());
        Ok(())
    }

    #[test]
    fn replan_legacy_claim_receipts_keep_their_slots_and_do_not_invent_target_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let key = ContentHash::from_bytes([
            18, 79, 185, 233, 35, 87, 189, 125, 76, 18, 171, 25, 133, 88, 204, 218, 182, 192, 48,
            149, 42, 137, 188, 91, 68, 28, 46, 231, 168, 124, 245, 172,
        ]);
        assert_eq!(legacy_claim_read_key_v1(), key);
        for outcome in [
            None,
            Some(ResearchAccessOutcome::Completed),
            Some(ResearchAccessOutcome::Unavailable),
        ] {
            let mut state = ReplanResearchCheckpoint::new(
                TaskStepId::from_bytes([1; 32]),
                SnapshotId::from_bytes([2; 32]),
                &TaskReplanReason::try_from_string("historical claim".to_owned())?,
                "find original",
            )?;
            state.work =
                state
                    .work
                    .with_restored_accesses(vec![a3_domain::ResearchAccessAttempt {
                        question: ResearchQuestionId::FIRST,
                        scope: ContentHash::from_bytes([2; 32]),
                        key,
                        kind: ResearchAccessKind::Inspect,
                        starts: 1,
                        outcome,
                    }])?;
            for id in [1, 2, 3] {
                let action = AgentAction::Inspect(a3_domain::AgentInspectAction::new(
                    a3_domain::AgentInspectTarget::Claim(a3_domain::ModuleCardClaimId::from_bytes(
                        [id; 32],
                    )),
                ));
                assert_eq!(
                    state.validate_read(&action),
                    Err(ReplanReadRejection::AmbiguousLegacyClaim)
                );
            }
            for query in ["original", "caller", "callee"] {
                let action = AgentAction::Search(a3_domain::AgentSearchAction::new(
                    a3_domain::AgentSearchQuery::try_from_string(query.to_owned())?,
                    a3_domain::AgentSearchLimit::new(5)?,
                ));
                assert!(state.permits(&action));
                state.record_read(&action, true)?;
            }
            assert_eq!(state.reads(), 4);
            assert_eq!(state.work.accesses()[0].key, key);
            assert_eq!(state.work.accesses()[0].starts, 1);
            assert!(!state.work.ready_to_finish());
        }
        Ok(())
    }

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
            assert_eq!(
                checkpoint.validate_read(&action),
                Err(if checkpoint.reads() == 4 {
                    ReplanReadRejection::BudgetExhausted
                } else {
                    ReplanReadRejection::RepeatedRead
                })
            );
        }
        assert_eq!(checkpoint.clone().reads(), 4);
        assert!(
            !checkpoint.work.ready_to_finish(),
            "four reads cannot answer a question"
        );
        Ok(())
    }
}
