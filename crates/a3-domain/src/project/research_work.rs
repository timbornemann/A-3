//! Bounded research state. Model interpretations are never promoted to verified facts.
use super::{
    AskResearchSourceId, ContentHash, FileRevision, ResearchAccessAttempt, ResearchAccessKind,
    ResearchAccessOutcome, SourceRange,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

/// Maximum number of stable questions in one research contract.
pub const MAX_RESEARCH_QUESTIONS: usize = 32;

/// Core-assigned position in an immutable research contract (not a tool capability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResearchQuestionId(u16);
impl ResearchQuestionId {
    /// First position in every nonempty research contract.
    pub const FIRST: Self = Self(1);
    /// Validates a previously issued question position.
    pub fn new(value: u16) -> Result<Self, ResearchWorkError> {
        if value == 0 || usize::from(value) > MAX_RESEARCH_QUESTIONS {
            return Err(ResearchWorkError::InvalidQuestion);
        }
        Ok(Self(value))
    }
    /// Returns the one-based position.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Whether resolving a question is necessary for the user's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchQuestionPriority {
    /// Explicit result requested by the user.
    Required,
    /// Prerequisite of another question, not an independent completion gate.
    Supporting,
    /// Additional detail that cannot delay required answers.
    Optional,
}

/// Existing repository behavior and proposed new design are different obligations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchQuestionKind {
    /// Existing behavior requires original current evidence.
    Repository,
    /// A proposed new interface or implementation decision.
    Design,
}

/// Non-authoritative initial decomposition, bound to literal user-request fragments.
#[derive(Clone, PartialEq, Eq)]
pub struct ResearchQuestionDraft {
    /// Literal fragment of the immutable user request.
    pub request_fragment: String,
    /// Public intended answer or design outcome.
    pub outcome: String,
    /// Requiredness, frozen at contract construction.
    pub priority: ResearchQuestionPriority,
    /// Existing-code investigation or new design.
    pub kind: ResearchQuestionKind,
    /// Earlier questions that must be resolved first.
    pub dependencies: Vec<ResearchQuestionId>,
}
impl fmt::Debug for ResearchQuestionDraft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResearchQuestionDraft")
            .field("priority", &self.priority)
            .field("kind", &self.kind)
            .field("dependencies", &self.dependencies)
            .finish_non_exhaustive()
    }
}

/// Original current source supporting an interpretation; no retained source blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchResultSource {
    /// Stable source identity issued by the safe research reader.
    pub source_id: AskResearchSourceId,
    /// Original content-addressed file revision.
    pub revision: FileRevision,
    /// Original inspected range, not a generated summary location.
    pub range: SourceRange,
}

/// Epistemic status, deliberately without a model-selectable VerifiedFact variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchResultKind {
    /// Source-supported interpretation, not a machine-verified fact.
    Interpretation,
    /// Explicit proposed design for new behavior.
    DesignDecision,
    /// Honest limitation tied to a Core-recorded investigation boundary.
    BoundedUnknown,
}

/// Bounded public result whose original references are admitted by the source boundary.
#[derive(Clone, PartialEq, Eq)]
pub struct ResearchResult {
    kind: ResearchResultKind,
    text: String,
    sources: Vec<ResearchResultSource>,
    boundary: Option<ContentHash>,
}
impl ResearchResult {
    /// Creates a source-bound interpretation, design decision, or Core-bounded unknown.
    pub fn new(
        kind: ResearchResultKind,
        text: String,
        sources: Vec<ResearchResultSource>,
        boundary: Option<ContentHash>,
    ) -> Result<Self, ResearchWorkError> {
        if !valid_text(&text, 4096)
            || sources.len() > 32
            || (kind == ResearchResultKind::Interpretation && sources.is_empty())
            || (kind == ResearchResultKind::BoundedUnknown && boundary.is_none())
            || (kind != ResearchResultKind::BoundedUnknown && boundary.is_some())
        {
            return Err(ResearchWorkError::InvalidResult);
        }
        if sources
            .iter()
            .enumerate()
            .any(|(index, source)| sources[..index].contains(source))
        {
            return Err(ResearchWorkError::InvalidResult);
        }
        Ok(Self {
            kind,
            text,
            sources,
            boundary,
        })
    }
    #[must_use]
    /// Returns the epistemic classification.
    pub const fn kind(&self) -> ResearchResultKind {
        self.kind
    }
    #[must_use]
    /// Returns the bounded public conclusion.
    pub fn text(&self) -> &str {
        &self.text
    }
    #[must_use]
    /// Returns original evidence, never model-rewritten source.
    pub fn sources(&self) -> &[ResearchResultSource] {
        &self.sources
    }
    #[must_use]
    /// Returns the Core-issued publication scope of an exhausted investigation, if any.
    pub const fn boundary(&self) -> Option<ContentHash> {
        self.boundary
    }
}
impl fmt::Debug for ResearchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResearchResult")
            .field("kind", &self.kind)
            .field("sources", &self.sources.len())
            .finish_non_exhaustive()
    }
}

/// Status is changed only by the research aggregate, never by a model status field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchQuestionStatus {
    /// Not yet analyzed.
    Open,
    /// Currently selected investigation.
    Active,
    /// An admitted source-supported or design result exists.
    Answered,
    /// An admitted answer explicitly explains a bounded unknown.
    Limited,
    /// No further allowed access remains; not answered.
    Blocked,
    /// Underlying evidence or a prerequisite changed.
    Stale,
}

/// One stable question, with immutable definition and retained original result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchQuestion {
    id: ResearchQuestionId,
    draft: ResearchQuestionDraft,
    status: ResearchQuestionStatus,
    result: Option<ResearchResult>,
    attempts: BTreeSet<ContentHash>,
    exclusions: BTreeSet<ContentHash>,
}
impl ResearchQuestion {
    #[must_use]
    /// Returns the stable Core-assigned identity.
    pub const fn id(&self) -> ResearchQuestionId {
        self.id
    }
    #[must_use]
    /// Returns the immutable question definition.
    pub const fn definition(&self) -> &ResearchQuestionDraft {
        &self.draft
    }
    #[must_use]
    /// Returns the materialized state.
    pub const fn status(&self) -> ResearchQuestionStatus {
        self.status
    }
    #[must_use]
    /// Returns the last result, including historical evidence when stale.
    pub const fn result(&self) -> Option<&ResearchResult> {
        self.result.as_ref()
    }
    #[must_use]
    /// Returns canonical packet identities already analyzed.
    pub fn attempts(&self) -> &BTreeSet<ContentHash> {
        &self.attempts
    }
    #[must_use]
    /// Returns Core-recorded exhausted investigation paths.
    pub fn exclusions(&self) -> &BTreeSet<ContentHash> {
        &self.exclusions
    }
    #[must_use]
    /// Returns whether the current result can contribute to closure.
    pub const fn resolved(&self) -> bool {
        matches!(
            self.status,
            ResearchQuestionStatus::Answered | ResearchQuestionStatus::Limited
        )
    }
}

/// Materialized research contract. No execution, permission or Task Ledger authority.
#[derive(Clone, PartialEq, Eq)]
pub struct ResearchWorkState {
    objective: String,
    revision: u32,
    questions: Vec<ResearchQuestion>,
    accesses: Vec<ResearchAccessAttempt>,
}

/// Validated persistence input; never accepted from the model or WebView.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchQuestionCheckpoint {
    /// Immutable question definition.
    pub definition: ResearchQuestionDraft,
    /// Materialized Core status.
    pub status: ResearchQuestionStatus,
    /// Current or historical source-bound result.
    pub result: Option<ResearchResult>,
    /// Bounded canonical analysis identities, not raw packets.
    pub attempts: BTreeSet<ContentHash>,
    /// Core-issued exhausted access identities.
    pub exclusions: BTreeSet<ContentHash>,
}
impl fmt::Debug for ResearchWorkState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResearchWorkState")
            .field("revision", &self.revision)
            .field("questions", &self.questions.len())
            .finish_non_exhaustive()
    }
}
impl ResearchWorkState {
    /// Reconstitutes a bounded aggregate, checking state and dependency invariants again.
    pub fn restore(
        objective: String,
        revision: u32,
        checkpoints: Vec<ResearchQuestionCheckpoint>,
    ) -> Result<Self, ResearchWorkError> {
        if revision == 0 || revision > 65536 {
            return Err(ResearchWorkError::RevisionLimit);
        }
        let mut state = Self::new(
            objective,
            checkpoints.iter().map(|q| q.definition.clone()).collect(),
        )?;
        for (question, checkpoint) in state.questions.iter_mut().zip(checkpoints) {
            if checkpoint.attempts.len() > 24 || checkpoint.exclusions.len() > 48 {
                return Err(ResearchWorkError::AttemptLimit);
            }
            match checkpoint.status {
                ResearchQuestionStatus::Open
                    if checkpoint.result.is_some() || !checkpoint.attempts.is_empty() =>
                {
                    return Err(ResearchWorkError::InvalidTransition);
                }
                ResearchQuestionStatus::Active
                    if checkpoint.result.is_some() || checkpoint.attempts.is_empty() =>
                {
                    return Err(ResearchWorkError::InvalidTransition);
                }
                ResearchQuestionStatus::Answered | ResearchQuestionStatus::Limited => {
                    let result = checkpoint
                        .result
                        .as_ref()
                        .ok_or(ResearchWorkError::InvalidResult)?;
                    if (checkpoint.status == ResearchQuestionStatus::Limited)
                        != (result.kind() == ResearchResultKind::BoundedUnknown)
                    {
                        return Err(ResearchWorkError::InvalidResult);
                    }
                }
                _ => {}
            }
            if let Some(result) = &checkpoint.result {
                if (result.kind() == ResearchResultKind::DesignDecision)
                    != (question.draft.kind == ResearchQuestionKind::Design)
                {
                    return Err(ResearchWorkError::InvalidResult);
                }
                if checkpoint.status == ResearchQuestionStatus::Limited
                    && !result
                        .boundary()
                        .is_some_and(|b| checkpoint.exclusions.contains(&b))
                {
                    return Err(ResearchWorkError::InvalidResult);
                }
            }
            question.status = checkpoint.status;
            question.result = checkpoint.result;
            question.attempts = checkpoint.attempts;
            question.exclusions = checkpoint.exclusions;
        }
        if state.questions.iter().any(|q| {
            q.resolved()
                && q.draft
                    .dependencies
                    .iter()
                    .any(|id| !state.question(*id).is_some_and(ResearchQuestion::resolved))
        }) {
            return Err(ResearchWorkError::InvalidTransition);
        }
        state.revision = revision;
        Ok(state)
    }
    /// Freezes a bounded decomposition and assigns stable Core-owned identities.
    pub fn new(
        objective: String,
        drafts: Vec<ResearchQuestionDraft>,
    ) -> Result<Self, ResearchWorkError> {
        if !valid_text(&objective, 32 * 1024)
            || drafts.is_empty()
            || drafts.len() > MAX_RESEARCH_QUESTIONS
            || !drafts
                .iter()
                .any(|d| d.priority == ResearchQuestionPriority::Required)
        {
            return Err(ResearchWorkError::InvalidQuestion);
        }
        let mut questions = Vec::with_capacity(drafts.len());
        let mut outcomes = BTreeSet::new();
        for (index, draft) in drafts.into_iter().enumerate() {
            let position =
                u16::try_from(index + 1).map_err(|_| ResearchWorkError::InvalidQuestion)?;
            if !valid_text(&draft.outcome, 512)
                || !valid_text(&draft.request_fragment, 2048)
                || !objective.contains(&draft.request_fragment)
                || !outcomes.insert(
                    draft
                        .outcome
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase(),
                )
                || draft.dependencies.iter().any(|id| id.get() >= position)
                || draft.dependencies.iter().collect::<BTreeSet<_>>().len()
                    != draft.dependencies.len()
            {
                return Err(ResearchWorkError::InvalidQuestion);
            }
            questions.push(ResearchQuestion {
                id: ResearchQuestionId::new(position)?,
                draft,
                status: ResearchQuestionStatus::Open,
                result: None,
                attempts: BTreeSet::new(),
                exclusions: BTreeSet::new(),
            });
        }
        Ok(Self {
            objective,
            revision: 1,
            questions,
            accesses: Vec::new(),
        })
    }
    /// Restores adapter-owned access receipts, without granting model or UI authority.
    pub fn with_restored_accesses(
        mut self,
        accesses: Vec<ResearchAccessAttempt>,
    ) -> Result<Self, ResearchWorkError> {
        if accesses.len() > 256 {
            return Err(ResearchWorkError::AttemptLimit);
        }
        for (i, access) in accesses.iter().enumerate() {
            access.validate()?;
            if self.question(access.question).is_none()
                || accesses[..i].iter().any(|other| {
                    other.question == access.question
                        && other.scope == access.scope
                        && other.key == access.key
                })
            {
                return Err(ResearchWorkError::InvalidTransition);
            }
        }
        self.accesses = accesses;
        Ok(self)
    }
    /// Returns bounded content-free attempts and their distinct Core outcomes.
    #[must_use]
    pub fn accesses(&self) -> &[ResearchAccessAttempt] {
        &self.accesses
    }

    /// Checks only current-scope, same-question deterministic negative receipts.
    #[must_use]
    pub fn access_excluded(
        &self,
        question: ResearchQuestionId,
        scope: ContentHash,
        key: ContentHash,
    ) -> bool {
        self.accesses.iter().any(|a| {
            a.question == question && a.scope == scope && a.key == key && a.excludes_repeat()
        })
    }

    /// Records an actual start before the tool is called. A crash leaves an unfinished receipt.
    pub fn begin_access(
        &mut self,
        question: ResearchQuestionId,
        scope: ContentHash,
        key: ContentHash,
        kind: ResearchAccessKind,
    ) -> Result<bool, ResearchWorkError> {
        if self.next_question() != Some(question) {
            return Err(ResearchWorkError::InvalidTransition);
        }
        let existing = self
            .accesses
            .iter()
            .position(|a| a.question == question && a.scope == scope && a.key == key);
        if existing.is_none() && self.accesses.len() >= 256 {
            return Err(ResearchWorkError::AttemptLimit);
        }
        if let Some(index) = existing {
            if self.accesses[index].kind != kind {
                return Err(ResearchWorkError::InvalidTransition);
            }
            if self.accesses[index].excludes_repeat() {
                return Ok(false);
            }
            if self.accesses[index].starts >= 256 {
                return Err(ResearchWorkError::AttemptLimit);
            }
        }
        self.ensure_can_advance()?;
        if let Some(index) = existing {
            self.accesses[index].starts += 1;
            self.accesses[index].outcome = None;
        } else {
            self.accesses.push(ResearchAccessAttempt {
                question,
                scope,
                key,
                kind,
                starts: 1,
                outcome: None,
            });
        }
        self.advance()?;
        Ok(true)
    }

    /// Acknowledges only a started access; no result or question can be completed by this method.
    pub fn finish_access(
        &mut self,
        question: ResearchQuestionId,
        scope: ContentHash,
        key: ContentHash,
        outcome: ResearchAccessOutcome,
    ) -> Result<(), ResearchWorkError> {
        let index = self
            .accesses
            .iter()
            .position(|a| a.question == question && a.scope == scope && a.key == key)
            .ok_or(ResearchWorkError::InvalidTransition)?;
        if self.accesses[index].outcome == Some(outcome) {
            return Ok(());
        }
        if self.accesses[index].outcome.is_some() {
            return Err(ResearchWorkError::InvalidTransition);
        }
        let mut receipt = self.accesses[index].clone();
        receipt.outcome = Some(outcome);
        receipt.validate()?;
        self.ensure_can_advance()?;
        self.accesses[index].outcome = Some(outcome);
        self.advance()
    }
    #[must_use]
    /// Returns the original, unchanged user request.
    pub fn objective(&self) -> &str {
        &self.objective
    }
    #[must_use]
    /// Returns the monotone work-state revision.
    pub const fn revision(&self) -> u32 {
        self.revision
    }
    #[must_use]
    /// Returns definitions and current results in stable order.
    pub fn questions(&self) -> &[ResearchQuestion] {
        &self.questions
    }
    #[must_use]
    /// Looks up only an existing contract question.
    pub fn question(&self, id: ResearchQuestionId) -> Option<&ResearchQuestion> {
        self.questions.get(usize::from(id.get() - 1))
    }
    /// Selects required work and its transitive prerequisites, never optional scope drift.
    #[must_use]
    pub fn next_question(&self) -> Option<ResearchQuestionId> {
        let mut needed = BTreeSet::new();
        for question in self.questions.iter().rev() {
            if !question.resolved()
                && (question.draft.priority == ResearchQuestionPriority::Required
                    || needed.contains(&question.id))
            {
                needed.insert(question.id);
                needed.extend(question.draft.dependencies.iter().copied());
            }
        }
        self.questions
            .iter()
            .find(|q| {
                needed.contains(&q.id)
                    && !q.resolved()
                    && q.status != ResearchQuestionStatus::Blocked
                    && q.draft
                        .dependencies
                        .iter()
                        .all(|id| self.question(*id).is_some_and(ResearchQuestion::resolved))
            })
            .map(ResearchQuestion::id)
    }
    /// Records exactly one substantive analysis of a canonical current evidence packet.
    pub fn begin_analysis(
        &mut self,
        id: ResearchQuestionId,
        packet: ContentHash,
    ) -> Result<(), ResearchWorkError> {
        if self.next_question() != Some(id) {
            return Err(ResearchWorkError::InvalidTransition);
        }
        self.ensure_can_advance()?;
        let question = self
            .questions
            .get_mut(usize::from(id.get() - 1))
            .ok_or(ResearchWorkError::InvalidQuestion)?;
        if question.attempts.contains(&packet) {
            return Err(ResearchWorkError::RepeatedAnalysis);
        }
        if question.attempts.len() >= 24 {
            return Err(ResearchWorkError::AttemptLimit);
        }
        question.attempts.insert(packet);
        question.status = ResearchQuestionStatus::Active;
        question.result = None;
        self.advance()
    }
    /// Admits a result only for open dependency-ready work; an answer cannot erase prior work.
    pub fn resolve(
        &mut self,
        id: ResearchQuestionId,
        result: ResearchResult,
    ) -> Result<(), ResearchWorkError> {
        let question = self
            .question(id)
            .ok_or(ResearchWorkError::InvalidQuestion)?;
        if question.resolved() {
            return if question.result.as_ref() == Some(&result) {
                Ok(())
            } else {
                Err(ResearchWorkError::InvalidTransition)
            };
        }
        if question.status == ResearchQuestionStatus::Blocked
            || question
                .draft
                .dependencies
                .iter()
                .any(|id| !self.question(*id).is_some_and(ResearchQuestion::resolved))
            || ((result.kind == ResearchResultKind::DesignDecision)
                != (question.draft.kind == ResearchQuestionKind::Design))
            || (result.kind == ResearchResultKind::BoundedUnknown
                && !result
                    .boundary
                    .is_some_and(|key| question.exclusions.contains(&key)))
        {
            return Err(ResearchWorkError::InvalidResult);
        }
        self.ensure_can_advance()?;
        let question = self
            .questions
            .get_mut(usize::from(id.get() - 1))
            .ok_or(ResearchWorkError::InvalidQuestion)?;
        question.status = if result.kind == ResearchResultKind::BoundedUnknown {
            ResearchQuestionStatus::Limited
        } else {
            ResearchQuestionStatus::Answered
        };
        question.result = Some(result);
        self.advance()
    }
    /// Records an exhausted bounded access path supplied by the Core, not by the model.
    pub fn exclude(
        &mut self,
        id: ResearchQuestionId,
        access: ContentHash,
    ) -> Result<bool, ResearchWorkError> {
        let current = self
            .question(id)
            .ok_or(ResearchWorkError::InvalidQuestion)?;
        if current.exclusions.contains(&access) {
            return Ok(false);
        }
        self.ensure_can_advance()?;
        let question = self
            .questions
            .get_mut(usize::from(id.get() - 1))
            .ok_or(ResearchWorkError::InvalidQuestion)?;
        if question.resolved() || question.exclusions.len() >= 48 {
            return Err(ResearchWorkError::InvalidTransition);
        }
        let added = question.exclusions.insert(access);
        if added {
            self.advance()?;
        }
        Ok(added)
    }
    /// Blocks exhausted work without claiming that the user's question has been answered.
    pub fn block(&mut self, id: ResearchQuestionId) -> Result<(), ResearchWorkError> {
        if self
            .question(id)
            .is_some_and(|q| q.status == ResearchQuestionStatus::Blocked)
        {
            return Ok(());
        }
        self.ensure_can_advance()?;
        let question = self
            .questions
            .get_mut(usize::from(id.get() - 1))
            .ok_or(ResearchWorkError::InvalidQuestion)?;
        if question.resolved() {
            return Err(ResearchWorkError::InvalidTransition);
        }
        question.status = ResearchQuestionStatus::Blocked;
        self.advance()
    }
    /// Invalidates dependent answers before they can reenter model context.
    pub fn revalidate(&mut self, current: &[FileRevision]) -> Result<bool, ResearchWorkError> {
        self.revalidate_in_scope(current, None)
    }

    /// Negative investigation receipts require their original publication scope in addition
    /// to file freshness. Without that scope an absence-derived result is conservatively stale.
    pub fn revalidate_in_scope(
        &mut self,
        current: &[FileRevision],
        scope: Option<ContentHash>,
    ) -> Result<bool, ResearchWorkError> {
        let mut stale = BTreeSet::new();
        for question in &self.questions {
            if question
                .result
                .as_ref()
                .is_some_and(|r| r.sources.iter().any(|s| !current.contains(&s.revision)))
                || question.result.as_ref().is_some_and(|r| {
                    r.kind == ResearchResultKind::BoundedUnknown
                        && (scope.is_none()
                            || r.boundary != scope
                            || !self
                                .accesses
                                .iter()
                                .any(|a| a.question == question.id && Some(a.scope) == scope))
                })
                || question
                    .draft
                    .dependencies
                    .iter()
                    .any(|id| stale.contains(id))
            {
                stale.insert(question.id);
            }
        }
        let changed = self
            .questions
            .iter()
            .any(|q| stale.contains(&q.id) && q.status != ResearchQuestionStatus::Stale);
        if changed {
            self.ensure_can_advance()?;
        }
        for question in &mut self.questions {
            if stale.contains(&question.id) && question.status != ResearchQuestionStatus::Stale {
                question.status = ResearchQuestionStatus::Stale;
                question.attempts.clear();
                question.exclusions.clear();
            }
        }
        if changed {
            self.advance()?;
        }
        Ok(changed)
    }
    #[must_use]
    /// Requires every required question to have an admitted current result.
    pub fn ready_to_finish(&self) -> bool {
        self.questions
            .iter()
            .filter(|q| q.draft.priority == ResearchQuestionPriority::Required)
            .all(ResearchQuestion::resolved)
    }
    /// A user choice may interrupt planning only after required repository work is resolved.
    /// This is not completion and gives a proposed design no execution or verification authority.
    #[must_use]
    pub fn can_request_design_choice(&self) -> bool {
        self.next_question()
            .and_then(|id| self.question(id))
            .is_some_and(|q| q.draft.kind == ResearchQuestionKind::Design)
            && self
                .questions
                .iter()
                .filter(|q| {
                    q.draft.priority == ResearchQuestionPriority::Required
                        && q.draft.kind == ResearchQuestionKind::Repository
                })
                .all(ResearchQuestion::resolved)
    }
    /// Number of resolved questions, independent of source reads and context deliveries.
    #[must_use]
    pub fn resolved_count(&self) -> usize {
        self.questions.iter().filter(|q| q.resolved()).count()
    }
    fn advance(&mut self) -> Result<(), ResearchWorkError> {
        self.ensure_can_advance()?;
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(ResearchWorkError::RevisionLimit)?;
        Ok(())
    }
    fn ensure_can_advance(&self) -> Result<(), ResearchWorkError> {
        if self.revision >= 65536 {
            Err(ResearchWorkError::RevisionLimit)
        } else {
            Ok(())
        }
    }
}

fn valid_text(text: &str, limit: usize) -> bool {
    !text.trim().is_empty()
        && text.len() <= limit
        && !text
            .chars()
            .any(|c| c == '\0' || (c.is_control() && !matches!(c, '\n' | '\r' | '\t')))
}

/// Content-free state transition failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchWorkError {
    /// Unknown identity or invalid immutable definition.
    InvalidQuestion,
    /// Missing source support or incompatible result kind.
    InvalidResult,
    /// Attempt to alter resolved or dependency-blocked work.
    InvalidTransition,
    /// The same canonical packet has already been analyzed.
    RepeatedAnalysis,
    /// The fixed per-question attempt bound was reached.
    AttemptLimit,
    /// The monotone state revision cannot advance.
    RevisionLimit,
}
impl fmt::Display for ResearchWorkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("research work state rejected the transition")
    }
}
impl Error for ResearchWorkError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RepositoryPath, SourcePosition};
    #[test]
    fn repository_results_cannot_complete_or_restore_a_design_obligation()
    -> Result<(), Box<dyn Error>> {
        let mut definition = draft("future design", ResearchQuestionPriority::Required);
        definition.kind = ResearchQuestionKind::Design;
        let scope = ContentHash::from_bytes([7; 32]);
        let mut state =
            ResearchWorkState::new("chain and log".to_owned(), vec![definition.clone()])?;
        state.exclude(ResearchQuestionId::FIRST, scope)?;
        for unsupported in [
            result()?,
            ResearchResult::new(
                ResearchResultKind::BoundedUnknown,
                "No matching original in the inspected scope".to_owned(),
                vec![],
                Some(scope),
            )?,
        ] {
            let before = state.clone();
            assert_eq!(
                state.resolve(ResearchQuestionId::FIRST, unsupported.clone()),
                Err(ResearchWorkError::InvalidResult)
            );
            assert_eq!(state, before);
            for status in [
                ResearchQuestionStatus::Answered,
                ResearchQuestionStatus::Limited,
                ResearchQuestionStatus::Stale,
            ] {
                assert!(
                    ResearchWorkState::restore(
                        "chain and log".to_owned(),
                        2,
                        vec![ResearchQuestionCheckpoint {
                            definition: definition.clone(),
                            status,
                            result: Some(unsupported.clone()),
                            attempts: BTreeSet::new(),
                            exclusions: BTreeSet::from([scope]),
                        }]
                    )
                    .is_err()
                );
            }
        }
        state.resolve(
            ResearchQuestionId::FIRST,
            ResearchResult::new(
                ResearchResultKind::DesignDecision,
                "Add a new read-only interface and test its error behavior".to_owned(),
                vec![],
                None,
            )?,
        )?;
        assert!(state.ready_to_finish());
        Ok(())
    }

    #[test]
    fn research_access_attempts_survive_restart_without_becoming_answers()
    -> Result<(), Box<dyn Error>> {
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![draft("log", ResearchQuestionPriority::Required)],
        )?;
        let id = ResearchQuestionId::FIRST;
        let scope = ContentHash::from_bytes([1; 32]);
        let key = ContentHash::from_bytes([2; 32]);
        assert!(
            state
                .finish_access(id, scope, key, ResearchAccessOutcome::Completed)
                .is_err()
        );
        assert!(state.begin_access(id, scope, key, ResearchAccessKind::LiteralSearch)?);
        assert_eq!(state.accesses()[0].outcome, None);
        // An interrupted read is eligible again; starts alone never suppress it.
        assert!(state.begin_access(id, scope, key, ResearchAccessKind::LiteralSearch)?);
        state.finish_access(id, scope, key, ResearchAccessOutcome::Unavailable)?;
        assert!(state.begin_access(id, scope, key, ResearchAccessKind::LiteralSearch)?);
        assert_eq!(state.accesses()[0].starts, 3);
        state.finish_access(id, scope, key, ResearchAccessOutcome::NoMatch)?;
        let saved = state.clone();
        assert!(!state.begin_access(id, scope, key, ResearchAccessKind::LiteralSearch)?);
        assert_eq!(
            state.begin_access(id, scope, key, ResearchAccessKind::Directory),
            Err(ResearchWorkError::InvalidTransition)
        );
        assert_eq!(state, saved);
        assert!(!state.ready_to_finish());
        assert!(state.questions()[0].exclusions().is_empty()); // not a whole-question boundary
        assert!(
            state
                .finish_access(id, scope, key, ResearchAccessOutcome::Completed)
                .is_err()
        );
        let new_scope = ContentHash::from_bytes([3; 32]);
        assert!(state.begin_access(id, new_scope, key, ResearchAccessKind::LiteralSearch)?);
        state.finish_access(id, new_scope, key, ResearchAccessOutcome::Completed)?;
        // Original source bytes are volatile, so a successful read may hydrate the cache again.
        assert!(state.begin_access(id, new_scope, key, ResearchAccessKind::LiteralSearch)?);
        let mut duplicate = state.accesses().to_vec();
        duplicate.push(duplicate[0].clone());
        assert!(state.clone().with_restored_accesses(duplicate).is_err());
        let mut corrupt = state.accesses().to_vec();
        corrupt[0].kind = ResearchAccessKind::Changes;
        assert!(state.clone().with_restored_accesses(corrupt).is_err());
        assert_eq!(
            state
                .clone()
                .with_restored_accesses(state.accesses().to_vec())?,
            state
        );
        Ok(())
    }

    #[test]
    fn research_access_limits_are_atomic_and_do_not_erase_prior_receipts()
    -> Result<(), Box<dyn Error>> {
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![draft("log", ResearchQuestionPriority::Required)],
        )?;
        let id = ResearchQuestionId::FIRST;
        let scope = ContentHash::from_bytes([1; 32]);
        for n in 0..=255u8 {
            state.begin_access(
                id,
                scope,
                ContentHash::from_bytes([n; 32]),
                ResearchAccessKind::Inspect,
            )?;
        }
        let saved = state.clone();
        assert_eq!(
            state.begin_access(
                id,
                ContentHash::from_bytes([2; 32]),
                scope,
                ResearchAccessKind::Inspect
            ),
            Err(ResearchWorkError::AttemptLimit)
        );
        assert_eq!(state, saved);
        state.revision = 65536;
        let saved = state.clone();
        assert_eq!(
            state.finish_access(id, scope, scope, ResearchAccessOutcome::Completed),
            Err(ResearchWorkError::RevisionLimit)
        );
        assert_eq!(
            state.begin_access(id, scope, scope, ResearchAccessKind::Inspect),
            Err(ResearchWorkError::RevisionLimit)
        );
        assert_eq!(state, saved);
        Ok(())
    }
    #[test]
    fn revision_exhaustion_never_partially_mutates_work() -> Result<(), Box<dyn Error>> {
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![draft("log", ResearchQuestionPriority::Required)],
        )?;
        state.revision = 65536;
        let before = state.clone();
        let id = ResearchQuestionId::FIRST;
        let key = ContentHash::from_bytes([3; 32]);
        assert_eq!(
            state.begin_analysis(id, key),
            Err(ResearchWorkError::RevisionLimit)
        );
        assert_eq!(state, before);
        assert_eq!(
            state.exclude(id, key),
            Err(ResearchWorkError::RevisionLimit)
        );
        assert_eq!(state, before);
        assert_eq!(state.block(id), Err(ResearchWorkError::RevisionLimit));
        assert_eq!(state, before);
        assert_eq!(
            state.resolve(id, result()?),
            Err(ResearchWorkError::RevisionLimit)
        );
        assert_eq!(state, before);
        state.revision = 65535;
        state.resolve(id, result()?)?;
        let answered = state.clone();
        assert_eq!(state.revalidate(&[]), Err(ResearchWorkError::RevisionLimit));
        assert_eq!(state, answered);
        assert!(ResearchWorkState::restore("chain and log".to_owned(), 65537, vec![]).is_err());
        Ok(())
    }
    fn draft(outcome: &str, priority: ResearchQuestionPriority) -> ResearchQuestionDraft {
        ResearchQuestionDraft {
            request_fragment: "chain and log".to_owned(),
            outcome: outcome.to_owned(),
            priority,
            kind: ResearchQuestionKind::Repository,
            dependencies: vec![],
        }
    }
    fn result() -> Result<ResearchResult, Box<dyn Error>> {
        Ok(ResearchResult::new(
            ResearchResultKind::Interpretation,
            "log target in constructor".to_owned(),
            vec![ResearchResultSource {
                source_id: AskResearchSourceId::from_bytes([1; 32]),
                revision: FileRevision::new(
                    RepositoryPath::try_from_bytes(b"audit.py".to_vec())?,
                    ContentHash::from_bytes([2; 32]),
                ),
                range: SourceRange::new(
                    0,
                    10,
                    SourcePosition::new(0, 0),
                    SourcePosition::new(0, 10),
                )?,
            }],
            None,
        )?)
    }
    #[test]
    fn required_log_cannot_disappear_and_optional_registration_does_not_block()
    -> Result<(), Box<dyn Error>> {
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![
                draft("chain", ResearchQuestionPriority::Required),
                draft("log", ResearchQuestionPriority::Required),
                draft("registration", ResearchQuestionPriority::Optional),
            ],
        )?;
        state.resolve(ResearchQuestionId::new(1)?, result()?)?;
        assert!(!state.ready_to_finish());
        assert_eq!(state.next_question(), Some(ResearchQuestionId::new(2)?));
        state.resolve(ResearchQuestionId::new(2)?, result()?)?;
        assert!(state.ready_to_finish());
        assert_eq!(state.next_question(), None);
        Ok(())
    }
    #[test]
    fn same_packet_cannot_restart_analysis_or_count_as_resolution() -> Result<(), Box<dyn Error>> {
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![draft("chain", ResearchQuestionPriority::Required)],
        )?;
        let id = ResearchQuestionId::new(1)?;
        state.begin_analysis(id, ContentHash::from_bytes([3; 32]))?;
        assert_eq!(
            state.begin_analysis(id, ContentHash::from_bytes([3; 32])),
            Err(ResearchWorkError::RepeatedAnalysis)
        );
        assert_eq!(state.resolved_count(), 0);
        assert!(!state.ready_to_finish());
        Ok(())
    }
    #[test]
    fn model_cannot_declare_an_unexamined_question_unknown_or_design() -> Result<(), Box<dyn Error>>
    {
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![draft("chain", ResearchQuestionPriority::Required)],
        )?;
        let id = ResearchQuestionId::new(1)?;
        let unknown = ResearchResult::new(
            ResearchResultKind::BoundedUnknown,
            "not known".to_owned(),
            vec![],
            Some(ContentHash::from_bytes([8; 32])),
        )?;
        assert_eq!(
            state.resolve(id, unknown),
            Err(ResearchWorkError::InvalidResult)
        );
        let design = ResearchResult::new(
            ResearchResultKind::DesignDecision,
            "invent a path".to_owned(),
            vec![],
            None,
        )?;
        assert_eq!(
            state.resolve(id, design),
            Err(ResearchWorkError::InvalidResult)
        );
        assert!(!state.ready_to_finish());
        Ok(())
    }
    #[test]
    fn changes_reopen_only_dependent_answers() -> Result<(), Box<dyn Error>> {
        let mut dependency = draft("log", ResearchQuestionPriority::Required);
        dependency.dependencies.push(ResearchQuestionId::new(1)?);
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![
                draft("chain", ResearchQuestionPriority::Required),
                dependency,
            ],
        )?;
        state.resolve(ResearchQuestionId::new(1)?, result()?)?;
        state.resolve(ResearchQuestionId::new(2)?, result()?)?;
        let revisions = result()?
            .sources()
            .iter()
            .map(|s| s.revision.clone())
            .collect::<Vec<_>>();
        assert!(!state.revalidate(&revisions)?);
        assert!(state.revalidate(&[])?);
        assert!(!state.ready_to_finish());
        assert!(
            state
                .questions()
                .iter()
                .all(|q| q.status() == ResearchQuestionStatus::Stale)
        );
        Ok(())
    }

    #[test]
    fn new_design_is_not_missing_repository_evidence_and_choices_do_not_finish_work()
    -> Result<(), Box<dyn Error>> {
        let mut design = draft("CSV duplicate policy", ResearchQuestionPriority::Required);
        design.kind = ResearchQuestionKind::Design;
        design.dependencies = vec![ResearchQuestionId::new(1)?];
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![
                draft("existing CLI", ResearchQuestionPriority::Required),
                design,
            ],
        )?;
        assert!(!state.can_request_design_choice());
        state.resolve(ResearchQuestionId::new(1)?, result()?)?;
        assert!(state.can_request_design_choice());
        assert!(!state.ready_to_finish());
        state.resolve(
            ResearchQuestionId::new(2)?,
            ResearchResult::new(
                ResearchResultKind::DesignDecision,
                "Reject duplicate rows before applying changes".to_owned(),
                vec![],
                None,
            )?,
        )?;
        assert!(state.ready_to_finish());
        Ok(())
    }

    #[test]
    fn blocked_question_does_not_starve_independent_work_and_restore_keeps_attempts()
    -> Result<(), Box<dyn Error>> {
        let mut state = ResearchWorkState::new(
            "chain and log".to_owned(),
            vec![
                draft("chain", ResearchQuestionPriority::Required),
                draft("log", ResearchQuestionPriority::Required),
            ],
        )?;
        let first = ResearchQuestionId::new(1)?;
        state.begin_analysis(first, ContentHash::from_bytes([3; 32]))?;
        state.block(first)?;
        assert_eq!(state.next_question(), Some(ResearchQuestionId::new(2)?));
        state.resolve(ResearchQuestionId::new(2)?, result()?)?;
        assert!(!state.ready_to_finish());
        assert_eq!(state.next_question(), None);
        let mut checkpoints = state
            .questions()
            .iter()
            .map(|q| ResearchQuestionCheckpoint {
                definition: q.definition().clone(),
                status: q.status(),
                result: q.result().cloned(),
                attempts: q.attempts().clone(),
                exclusions: q.exclusions().clone(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ResearchWorkState::restore(
                state.objective().to_owned(),
                state.revision(),
                checkpoints.clone()
            )?,
            state
        );
        checkpoints[1].result = None;
        assert_eq!(
            ResearchWorkState::restore(state.objective().to_owned(), state.revision(), checkpoints),
            Err(ResearchWorkError::InvalidResult)
        );
        Ok(())
    }
}
