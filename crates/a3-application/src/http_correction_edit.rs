//! Cold-bound HTTP source correction through the existing finite controller.
use crate::{
    AdvanceAgentController, AgentActionStore, AgentControllerControl, AgentControllerError,
    AgentControllerSignal, AgentSourceReadControl, AgentSourceReadFailure, AgentSourceReader,
    AppendRunEvent, EditPhaseAnchor, EditPhaseReservationFailure, HttpContextIntentAnchor,
    HttpContextIntentReply, HttpCorrectionPlanAnchor, HttpCorrectionPlanDecision,
    HttpCorrectionPlanFailure, HttpCorrectionPlanProposal, HttpCorrectionPlanStore,
    PreparedEditPhase, RunJournalStore, RunJournalStoreFailure, StoredTaskLedger,
    TaskLedgerStoreVersion, derive_http_correction_material_plan,
};
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunId, AgentRunTimestamp, ContentHash, EvidenceRef,
    FileRevision, GoalContract, GoalContractReference, LoopbackExpectedBody,
    LoopbackHttpMismatchDiagnostic, ModelProfile, ProjectIdentity, PublishedIndex, RunEvent,
    RunEventSequence, SnapshotId, SourcePosition, SourceRange, TaskLedger, TaskLedgerRevision,
    TaskLedgerTimestamp, TaskStepAttemptNumber, TaskStepAttemptOutcome, TaskStepId, TaskStepStatus,
    VerificationTarget,
};
use std::{error::Error, fmt, time::Instant};

/// Native immutable origin selected by the store from the exact applied correction revision.
/// This is checked data, never source freshness or execution authority by itself.
#[derive(Debug, Clone)]
pub struct HttpCorrectionOrigin {
    /// Original current-job source and failure binding.
    pub anchor: HttpCorrectionPlanAnchor,
    /// Actual Direct or Context producer; descriptive provenance, never execution authority.
    pub producer: HttpCorrectionProducer,
    /// Original cleaned physical mismatch, not successful HTTP Evidence.
    pub diagnostic: LoopbackHttpMismatchDiagnostic,
    /// Exact material revision event derived from the same job.
    pub applied_event: RunEvent,
}

/// Closed provenance of the material correction. Every variant still requires current replay
/// and authenticated durable history before it can be used by Apply/activation.
#[derive(Debug, Clone)]
pub enum HttpCorrectionProducer {
    /// A genuine canonical V4 Focus, with no S1 read substituted for its model decision.
    DirectV4Focus {
        /// Core-bound V4 job that produced the retained Focus.
        job: ContentHash,
        /// Typed proposal; current sources and durable history still require validation.
        proposal: HttpCorrectionPlanProposal,
    },
    /// A genuine accepted S1 parent, actual read R and canonical Ready Context intention.
    ContextV1Intent {
        /// Original failure, actual S1 read and supplied source-range coordinates.
        anchor: Box<HttpContextIntentAnchor>,
        /// Core-bound Context job, distinct from its parent V4 job.
        job: ContentHash,
        /// Canonical V4 S1 proposal that caused the actual read.
        parent_proposal: HttpCorrectionPlanProposal,
        /// Retained planning reply; validation requires a non-null intention.
        reply: HttpContextIntentReply,
        /// Accepted actual provider-wire digest authenticated by the store.
        accepted_wire: ContentHash,
    },
}
impl HttpCorrectionProducer {
    /// Descriptive identity only; this getter grants no source freshness or action authority.
    pub const fn producing_job(&self) -> ContentHash {
        match self {
            Self::DirectV4Focus { job, .. } | Self::ContextV1Intent { job, .. } => *job,
        }
    }
    /// Validates the closed producer shape and derives P=F or genuine R=F+1.
    /// Current originals, accepted wire and actual journal/acquisition authority remain separate.
    pub fn application_tail(
        &self,
        root: &HttpCorrectionPlanAnchor,
    ) -> Result<RunEventSequence, HttpCorrectionPlanFailure> {
        use HttpCorrectionPlanFailure as E;
        let bounded = |range: SourceRange| {
            !range.is_empty()
                && range.len() <= 8192
                && range
                    .end_position()
                    .row()
                    .checked_sub(range.start_position().row())
                    .and_then(|n| n.checked_add(u32::from(range.end_position().column() > 0)))
                    .is_some_and(|n| n <= 64)
        };
        match self {
            Self::DirectV4Focus { job, proposal } => {
                let HttpCorrectionPlanDecision::FocusLine(focus) = proposal.decision() else {
                    return Err(E::Binding);
                };
                let first = root
                    .source_range
                    .start_position()
                    .row()
                    .checked_add(1)
                    .ok_or(E::Binding)?;
                let last = root
                    .source_range
                    .end_position()
                    .row()
                    .checked_add(u32::from(root.source_range.end_position().column() > 0))
                    .ok_or(E::Binding)?;
                if proposal.job_digest() != *job
                    || !bounded(root.source_range)
                    || focus.get() < first
                    || focus.get() > last
                {
                    return Err(E::Binding);
                }
                Ok(root.sequence)
            }
            Self::ContextV1Intent {
                anchor,
                job,
                parent_proposal,
                reply,
                ..
            } => {
                let HttpCorrectionPlanDecision::NeedOfferedSource(offer) =
                    parent_proposal.decision()
                else {
                    return Err(E::Binding);
                };
                let parent_bytes = parent_proposal.canonical_retained_bytes()?;
                if &anchor.parent != root
                    || parent_proposal.job_digest() != anchor.parent_job
                    || root.source_offer.as_ref() != Some(offer.as_ref())
                    || anchor.parent_proposal_digest.as_bytes()
                        != blake3::hash(&parent_bytes).as_bytes()
                    || reply.job_digest() != *job
                    || reply.intent().is_none()
                    || root.sequence.get().checked_add(1) != Some(anchor.read_tail.get())
                    || anchor.read_at < anchor.read_started_at
                    || anchor.primary_range != root.source_range
                    || anchor.primary_text != root.source_text
                    || !bounded(anchor.primary_range)
                    || !bounded(anchor.context_range)
                    || anchor.definition_range.is_empty()
                    || !anchor.primary_range.contains(anchor.definition_range)
                    || anchor
                        .definition_range
                        .start_position()
                        .row()
                        .checked_add(1)
                        != Some(anchor.focus.get())
                    || (anchor.context_range.start_byte() < anchor.primary_range.end_byte()
                        && anchor.primary_range.start_byte() < anchor.context_range.end_byte())
                {
                    return Err(E::Binding);
                }
                Ok(anchor.read_tail)
            }
        }
    }
}

/// Activates only the proven current HTTP correction; does not dispatch reads, models or patches.
#[derive(Clone, Copy)]
pub struct ActivateHttpCorrectionEdit<'a> {
    origins: &'a dyn HttpCorrectionPlanStore,
    journal: &'a dyn RunJournalStore,
    actions: &'a dyn AgentActionStore,
    reader: &'a dyn AgentSourceReader,
}
impl fmt::Debug for ActivateHttpCorrectionEdit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActivateHttpCorrectionEdit")
            .finish_non_exhaustive()
    }
}
impl<'a> ActivateHttpCorrectionEdit<'a> {
    /// Uses only the current origin, journal, atomic action and safe-source boundaries.
    #[must_use]
    pub const fn new(
        origins: &'a dyn HttpCorrectionPlanStore,
        journal: &'a dyn RunJournalStore,
        actions: &'a dyn AgentActionStore,
        reader: &'a dyn AgentSourceReader,
    ) -> Self {
        Self {
            origins,
            journal,
            actions,
            reader,
        }
    }

    /// Resumes an exact durable activation prefix and returns the bounded diagnosis only in Execute.
    /// Each successful edge updates the caller after its commit. A later failure retains that
    /// last committed prefix; no partial uncommitted Run/Ledger is exposed.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        version: &mut TaskLedgerStoreVersion,
        current: &PublishedIndex,
        profile: &ModelProfile,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<HttpFailureEditDiagnostic, HttpCorrectionEditFailure>
    where
        C: AgentControllerControl + AgentSourceReadControl,
    {
        use HttpCorrectionEditFailure as E;
        let started = Instant::now();
        // Three actual edges plus one current Execute read. No planning or retry loop.
        for _ in 0..4 {
            checkpoint(run, observed(started, observed_at)?, control)?;
            let stored = StoredTaskLedger::new(ledger.clone(), *version);
            let origin = self
                .origins
                .load_applied_http_correction_origin(
                    project,
                    goal,
                    &stored,
                    run,
                    current,
                    profile,
                    observed(started, observed_at)?,
                )
                .await
                .map_err(E::Origin)?;
            checkpoint(run, observed(started, observed_at)?, control)?;
            let a = &origin.anchor;
            let application_tail = origin
                .producer
                .application_tail(a)
                .map_err(|_| E::Binding)?;
            if !profile.executable_actions_enabled()
                || profile.reference() != a.profile
                || origin.applied_event.sequence().get()
                    != application_tail.get().checked_add(1).ok_or(E::Binding)?
                || origin.applied_event.run_id() != run.id()
                || origin.applied_event.snapshot_id() != a.snapshot
                || a.worktree != project.worktree().id()
                || a.run != run.id()
                || a.goal != goal.reference()
                || goal.reference() != ledger.goal_contract()
                || run.goal_contract() != goal.reference()
                || run.task_ledger_revision() != ledger.revision()
                || a.ledger.get().checked_add(1) != Some(ledger.revision().get())
                || a.index != current.run().id()
                || a.snapshot != current.run().snapshot_id()
                || a.snapshot != run.current_snapshot_id()
                || run.model_profile() != Some(a.profile)
                || ledger.pending_planning().is_some()
            {
                return Err(E::Binding);
            }
            let required = EvidenceRef::new(a.source.clone(), a.source_range);
            let inspection =
                crate::bound_edit_source::inspection_for_span(&required).map_err(|_| E::Binding)?;
            // This is Core revalidation of already supplied bytes, not a new model source acquisition.
            let page = self
                .reader
                .read_page(project, &a.source, &inspection, control)
                .await
                .map_err(E::Source)?;
            checkpoint(run, observed(started, observed_at)?, control)?;
            let failed = ledger.step(a.step).ok_or(E::Binding)?.definition();
            let job = origin.producer.producing_job();
            let (focus, intent) = match &origin.producer {
                HttpCorrectionProducer::DirectV4Focus { job, proposal } => {
                    let canonical = proposal
                        .canonical_retained_bytes()
                        .map_err(|_| E::Binding)?;
                    let checked = HttpCorrectionPlanProposal::verify_retained(
                        a,
                        goal,
                        failed,
                        &origin.diagnostic,
                        &page,
                        *job,
                        &canonical,
                    )
                    .map_err(|_| E::Binding)?;
                    if &checked != proposal {
                        return Err(E::Binding);
                    }
                    let HttpCorrectionPlanDecision::FocusLine(focus) = checked.decision() else {
                        return Err(E::Binding);
                    };
                    (focus, checked.intent().clone())
                }
                HttpCorrectionProducer::ContextV1Intent {
                    anchor,
                    job,
                    parent_proposal,
                    reply,
                    ..
                } => {
                    let required = EvidenceRef::new(a.source.clone(), anchor.context_range);
                    let inspection = crate::bound_edit_source::inspection_for_span(&required)
                        .map_err(|_| E::Binding)?;
                    // Revalidate S1 as well as primary; this grants no acquisition or byte credit.
                    let context = self
                        .reader
                        .read_page(project, &a.source, &inspection, control)
                        .await
                        .map_err(E::Source)?;
                    checkpoint(run, observed(started, observed_at)?, control)?;
                    let canonical = reply.canonical_retained_bytes().map_err(|_| E::Binding)?;
                    let checked = HttpContextIntentReply::verify_retained(
                        anchor,
                        parent_proposal,
                        goal,
                        failed,
                        &origin.diagnostic,
                        current,
                        profile,
                        &page,
                        &context,
                        *job,
                        &canonical,
                    )
                    .map_err(|_| E::Binding)?;
                    if &checked != reply {
                        return Err(E::Binding);
                    }
                    (anchor.focus, checked.intent().ok_or(E::Binding)?.clone())
                }
            };
            let material = derive_http_correction_material_plan(a, job, failed, focus, &intent)
                .map_err(|_| E::Binding)?;
            if material.event_id() != origin.applied_event.id()
                || ledger
                    .step(material.change().id())
                    .is_none_or(|step| step.definition() != material.change())
                || ledger
                    .step(material.proof().id())
                    .is_none_or(|step| step.definition() != material.proof())
            {
                return Err(E::Binding);
            }
            let definition = http_correction_definition_source(current, &a.source, focus)?;
            let source = definition_source(&page, &definition)?;
            let summary =
                mismatch_summary(failed.verification_spec().target(), &origin.diagnostic)?;
            if run.state() == AgentControllerState::Execute {
                let change = ledger.step(material.change().id()).ok_or(E::Binding)?;
                let [attempt] = change.attempts() else {
                    return Err(E::Binding);
                };
                if !change.is_active_plan_step()
                    || change.status() != TaskStepStatus::InProgress
                    || attempt.run_id() != run.id()
                    || attempt.number().get() != 1
                    || !matches!(attempt.outcome(), TaskStepAttemptOutcome::Active)
                {
                    return Err(E::Binding);
                }
                return Ok(HttpFailureEditDiagnostic {
                    goal: goal.reference(),
                    run: run.id(),
                    ledger: ledger.revision(),
                    snapshot: a.snapshot,
                    change: change.definition().id(),
                    attempt: attempt.number(),
                    source,
                    summary,
                });
            }
            let (signal, event, expected) = match run.state() {
                AgentControllerState::Replan => (
                    AgentControllerSignal::ReplanApplied,
                    material.activation_events()[0],
                    AgentControllerState::Localize,
                ),
                AgentControllerState::Localize => (
                    AgentControllerSignal::LocalizationComplete,
                    material.activation_events()[1],
                    AgentControllerState::Plan,
                ),
                AgentControllerState::Plan => (
                    AgentControllerSignal::PlanReady,
                    material.activation_events()[2],
                    AgentControllerState::Execute,
                ),
                _ => return Err(E::Binding),
            };
            let now = observed(started, observed_at)?;
            checkpoint(run, now, control)?;
            let mut next = run.clone();
            let mut next_ledger = ledger.clone();
            if signal == AgentControllerSignal::PlanReady {
                next_ledger
                    .start_step(
                        material.change().id(),
                        run.id(),
                        TaskLedgerTimestamp::from_unix_millis(now.unix_millis())
                            .map_err(|_| E::Binding)?,
                    )
                    .map_err(|_| E::Binding)?;
            }
            let advance = AdvanceAgentController
                .execute(&mut next, signal, event, a.snapshot, now, false)
                .map_err(E::Controller)?;
            if advance.state() != expected {
                return Err(E::BudgetExhausted);
            }
            checkpoint(run, observed(started, observed_at)?, control)?;
            if expected == AgentControllerState::Execute {
                let next_version = self
                    .actions
                    .commit_current_ledger_action(
                        project,
                        *version,
                        run.last_event_sequence(),
                        &next_ledger,
                        &next,
                        advance.event(),
                    )
                    .await
                    .map_err(E::Commit)?;
                *ledger = next_ledger;
                *version = next_version;
            } else {
                AppendRunEvent::new(self.journal)
                    .execute(project, run.last_event_sequence(), &next, advance.event())
                    .await
                    .map_err(E::Journal)?;
            }
            *run = next;
            checkpoint(run, observed(started, observed_at)?, control)?;
        }
        Err(E::Binding)
    }
}

fn observed(
    started: Instant,
    base: AgentRunTimestamp,
) -> Result<AgentRunTimestamp, HttpCorrectionEditFailure> {
    crate::model_wire::phase_elapsed_timestamp(started, base)
        .ok_or(HttpCorrectionEditFailure::Binding)
}
fn checkpoint(
    run: &AgentRun,
    now: AgentRunTimestamp,
    control: &dyn AgentControllerControl,
) -> Result<(), HttpCorrectionEditFailure> {
    if control.is_cancelled() {
        return Err(HttpCorrectionEditFailure::Cancelled);
    }
    if now < run.updated_at() {
        return Err(HttpCorrectionEditFailure::Binding);
    }
    if run
        .budget_exhaustion(now)
        .map_err(|_| HttpCorrectionEditFailure::Binding)?
        .is_some()
    {
        return Err(HttpCorrectionEditFailure::BudgetExhausted);
    }
    Ok(())
}
/// Selects the unique innermost current callable enclosing the retained physical Focus line.
/// The returned parsed span is descriptive metadata, never a read grant or proof of behavior.
pub fn http_correction_definition_source(
    current: &PublishedIndex,
    revision: &FileRevision,
    focus: a3_domain::AgentFileStartLine,
) -> Result<EvidenceRef, HttpCorrectionEditFailure> {
    if !current.publication().graph().files().contains(revision) {
        return Err(HttpCorrectionEditFailure::Binding);
    }
    select_definition(revision, focus, current.publication().graph().symbols())
}

fn select_definition(
    revision: &FileRevision,
    focus: a3_domain::AgentFileStartLine,
    symbols: &[a3_domain::GraphSymbol],
) -> Result<EvidenceRef, HttpCorrectionEditFailure> {
    use HttpCorrectionEditFailure as E;
    let row = focus.get().checked_sub(1).ok_or(E::Binding)?;
    let mut selected: Option<SourceRange> = None;
    for symbol in symbols {
        if symbol.revision() != revision
            || !matches!(
                symbol.parsed().kind(),
                a3_domain::SymbolKind::Function | a3_domain::SymbolKind::Method
            )
        {
            continue;
        }
        let range = symbol.parsed().declaration_range();
        let end = range.end_position();
        if range.is_empty()
            || range.start_position().row() > row
            || end.row() < row
            || (end.row() == row && end.column() == 0)
        {
            continue;
        }
        selected = Some(match selected {
            None => range,
            Some(previous) if previous != range && previous.contains(range) => range,
            Some(previous) if previous != range && range.contains(previous) => previous,
            // Duplicate or crossing innermost declarations are not a deterministic selection.
            Some(_) => return Err(E::Binding),
        });
    }
    let range = selected.ok_or(E::Binding)?;
    if range.end_byte().saturating_sub(range.start_byte()) as usize
        > crate::edit_phase::MAX_EDIT_BYTES
    {
        return Err(E::ContextCapacity);
    }
    Ok(EvidenceRef::new(revision.clone(), range))
}

/// Checks only the exact complete-line envelope of a parsed declaration and recorded read.
/// Current source freshness and normal acquisition/delivery remain separate checked boundaries.
pub fn http_correction_definition_page_matches(
    definition: &EvidenceRef,
    page: &EvidenceRef,
) -> bool {
    crate::bound_edit_source::definition_page_matches(definition, page)
}

fn definition_source(
    page: &crate::AgentSourcePage,
    definition: &EvidenceRef,
) -> Result<EvidenceRef, HttpCorrectionEditFailure> {
    use HttpCorrectionEditFailure as E;
    if page.revision() != definition.revision() || !page.range().contains(definition.range()) {
        // Do not pretend a partial original window is the complete enclosing implementation.
        return Err(E::ContextCapacity);
    }
    let first = definition.range().start_position().row();
    let end = definition.range().end_position();
    let until = end
        .row()
        .checked_add(u32::from(end.column() != 0))
        .ok_or(E::Binding)?;
    let page_row = page.start_line().get().checked_sub(1).ok_or(E::Binding)?;
    if page.range().start_position() != SourcePosition::new(page_row, 0) {
        return Err(E::Binding);
    }
    let mut offset = 0_usize;
    let mut selected = None;
    let mut finish = None;
    for (index, line) in page.text().split_inclusive('\n').enumerate() {
        let row = page_row
            .checked_add(u32::try_from(index).map_err(|_| E::Binding)?)
            .ok_or(E::Binding)?;
        let next = offset.checked_add(line.len()).ok_or(E::Binding)?;
        if row == first {
            selected = Some(offset);
        }
        if row.checked_add(1) == Some(until) {
            let position = if line.ends_with('\n') {
                SourcePosition::new(until, 0)
            } else {
                SourcePosition::new(row, u32::try_from(line.len()).map_err(|_| E::Binding)?)
            };
            finish = Some((next, position));
            break;
        }
        offset = next;
    }
    let start = selected.ok_or(E::ContextCapacity)?;
    let (finish, position) = finish.ok_or(E::ContextCapacity)?;
    if finish.checked_sub(start).ok_or(E::Binding)? > crate::edit_phase::MAX_EDIT_BYTES {
        return Err(E::ContextCapacity);
    }
    let base = usize::try_from(page.range().start_byte()).map_err(|_| E::Binding)?;
    let range = SourceRange::new(
        base.checked_add(start).ok_or(E::Binding)?,
        base.checked_add(finish).ok_or(E::Binding)?,
        SourcePosition::new(first, 0),
        position,
    )
    .map_err(|_| E::Binding)?;
    let actual = EvidenceRef::new(page.revision().clone(), range);
    if !http_correction_definition_page_matches(definition, &actual) {
        return Err(E::Binding);
    }
    Ok(actual)
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    use a3_domain::{
        AgentFileStartLine, GraphSymbol, LocalSymbolId, ParsedSymbol, RepositoryPath, SymbolId,
        SymbolKind, SymbolName,
    };
    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
    fn revision(seed: u8) -> TestResult<FileRevision> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(b"source.py".to_vec())?,
            ContentHash::from_bytes([seed; 32]),
        ))
    }
    fn range(
        start: usize,
        end: usize,
        first: u32,
        column: u32,
        last: u32,
        end_column: u32,
    ) -> TestResult<SourceRange> {
        Ok(SourceRange::new(
            start,
            end,
            SourcePosition::new(first, column),
            SourcePosition::new(last, end_column),
        )?)
    }
    fn symbol(
        seed: u8,
        revision: &FileRevision,
        kind: SymbolKind,
        span: SourceRange,
    ) -> TestResult<GraphSymbol> {
        Ok(GraphSymbol::new(
            SymbolId::from_bytes([seed; 32]),
            revision.clone(),
            ParsedSymbol::new(
                LocalSymbolId::new(u32::from(seed))?,
                kind,
                SymbolName::try_from_string(format!("callable_{seed}"))?,
                span,
                span,
            )?,
        ))
    }

    #[test]
    fn definition_focus_uses_current_unique_innermost_callable_including_first_and_last_line()
    -> TestResult {
        let current = revision(1)?;
        let stale = revision(2)?;
        let outer = range(10, 200, 1, 0, 10, 20)?;
        let inner = range(80, 130, 5, 4, 7, 18)?;
        let symbols = vec![
            symbol(1, &current, SymbolKind::Class, range(0, 220, 0, 0, 11, 0)?)?,
            symbol(2, &stale, SymbolKind::Method, inner)?,
            symbol(3, &current, SymbolKind::Method, outer)?,
            symbol(4, &current, SymbolKind::Function, inner)?,
        ];
        for line in [6, 7, 8] {
            assert_eq!(
                select_definition(&current, AgentFileStartLine::new(line)?, &symbols)?.range(),
                inner
            );
        }
        assert_eq!(
            select_definition(&current, AgentFileStartLine::new(2)?, &symbols)?.range(),
            outer
        );
        assert!(select_definition(&current, AgentFileStartLine::new(12)?, &symbols).is_err());
        let mut duplicate = symbols.clone();
        duplicate.push(symbol(5, &current, SymbolKind::Method, inner)?);
        assert!(select_definition(&current, AgentFileStartLine::new(7)?, &duplicate).is_err());
        let crossing = vec![
            symbol(
                6,
                &current,
                SymbolKind::Function,
                range(60, 120, 4, 0, 7, 12)?,
            )?,
            symbol(
                7,
                &current,
                SymbolKind::Method,
                range(80, 160, 5, 0, 9, 12)?,
            )?,
        ];
        assert!(select_definition(&current, AgentFileStartLine::new(7)?, &crossing).is_err());
        Ok(())
    }

    #[test]
    fn definition_page_expands_real_indentation_and_eol_without_accepting_partial_or_extra_lines()
    -> TestResult {
        let revision = revision(1)?;
        let text = "    def sample():\r\n        return value\r\n";
        let page = crate::AgentSourcePage::new(
            revision.clone(),
            range(100, 100 + text.len(), 10, 0, 12, 0)?,
            AgentFileStartLine::new(11)?,
            text.to_owned(),
            None,
            false,
        )?;
        let definition = EvidenceRef::new(
            revision.clone(),
            range(104, 100 + text.len() - 2, 10, 4, 11, 20)?,
        );
        let actual = definition_source(&page, &definition)?;
        assert_eq!(actual.range(), page.range());
        assert!(http_correction_definition_page_matches(
            &definition,
            &actual
        ));
        for span in [
            range(104, 100 + text.len(), 10, 4, 12, 0)?,
            range(100, 100 + text.len() + 1, 10, 0, 13, 0)?,
            range(100, 125, 10, 0, 11, 5)?,
        ] {
            assert!(!http_correction_definition_page_matches(
                &definition,
                &EvidenceRef::new(revision.clone(), span)
            ));
        }
        let different = EvidenceRef::new(super::definition_tests::revision(2)?, actual.range());
        assert!(!http_correction_definition_page_matches(
            &definition,
            &different
        ));
        let incomplete = crate::AgentSourcePage::new(
            revision,
            range(100, 119, 10, 0, 11, 0)?,
            AgentFileStartLine::new(11)?,
            "    def sample():\r\n".to_owned(),
            None,
            false,
        )?;
        assert!(matches!(
            definition_source(&incomplete, &definition),
            Err(HttpCorrectionEditFailure::ContextCapacity)
        ));
        Ok(())
    }
}

pub(crate) fn mismatch_summary(
    target: &VerificationTarget,
    diagnostic: &LoopbackHttpMismatchDiagnostic,
) -> Result<String, HttpCorrectionEditFailure> {
    use HttpCorrectionEditFailure as E;
    let VerificationTarget::LoopbackHttp(recipe) = target else {
        return Err(E::Binding);
    };
    let mismatch = diagnostic.mismatch();
    let expected = recipe
        .checks()
        .get(usize::from(mismatch.check_index()))
        .ok_or(E::Binding)?;
    let actual = mismatch.observation();
    let body = match expected.body() {
        LoopbackExpectedBody::Exact { hash, bytes } => format!(
            "bytes{}/{}{}",
            actual.body_bytes(),
            bytes,
            if hash != actual.body_hash() || bytes != actual.body_bytes() {
                " different"
            } else {
                ""
            }
        ),
        LoopbackExpectedBody::AnyBounded => format!("bytes{}/any", actual.body_bytes()),
    };
    let summary = format!(
        "{:?} {} {} status{}/{} {body}{}{}",
        mismatch.phase(),
        expected.probe().method().as_str(),
        expected.probe().path(),
        actual.status(),
        expected.status(),
        if expected.headers().plain_utf8 && !actual.headers().plain_utf8 {
            " plainUtf8Missing"
        } else {
            ""
        },
        if expected.headers().allow_get && !actual.headers().allow_get {
            " AllowGetMissing"
        } else {
            ""
        }
    );
    if summary.len() > 160 {
        return Err(E::ContextCapacity);
    }
    Ok(summary)
}

/// Actual retained HTTP mismatch bound to one genuine current correction attempt and definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpFailureEditDiagnostic {
    goal: GoalContractReference,
    run: AgentRunId,
    ledger: TaskLedgerRevision,
    snapshot: SnapshotId,
    change: TaskStepId,
    attempt: TaskStepAttemptNumber,
    source: EvidenceRef,
    summary: String,
}
impl HttpFailureEditDiagnostic {
    /// Complete current callable enclosing the retained Focus, expanded to full lines.
    /// This describes required evidence and does not grant a source read.
    pub const fn required_source(&self) -> &EvidenceRef {
        &self.source
    }
    /// Current complete source revision.
    pub const fn source_revision(&self) -> &FileRevision {
        self.source.revision()
    }
    /// Typed observed status/body/header mismatch only; never model intent text.
    pub fn summary(&self) -> &str {
        &self.summary
    }
    pub(crate) fn matches(&self, anchor: EditPhaseAnchor, phase: &PreparedEditPhase) -> bool {
        self.goal == anchor.goal()
            && self.run == anchor.run()
            && self.ledger == anchor.ledger()
            && self.snapshot == anchor.snapshot()
            && self.change == anchor.step()
            && self.attempt == anchor.attempt()
            && phase.targets_exact_span(&self.source)
    }
}

/// Closed preparation or activation failure with no model or patch dispatch.
#[derive(Debug)]
pub enum HttpCorrectionEditFailure {
    /// Cancellation won at a checked boundary; earlier committed prefixes remain durable.
    Cancelled,
    /// Current data or lineage cannot establish the exact source correction.
    Binding,
    /// Original Run hard budget or deadline is exhausted.
    BudgetExhausted,
    /// The complete selected definition or truthful mismatch cannot fit its bounded slot.
    ContextCapacity,
    /// Durable origin/current-state checks failed.
    Origin(EditPhaseReservationFailure),
    /// Fresh safe source revalidation failed.
    Source(AgentSourceReadFailure),
    /// The existing finite controller rejected an edge.
    Controller(AgentControllerError),
    /// A non-Execute edge failed its journal CAS.
    Journal(RunJournalStoreFailure),
    /// The actual correction start failed its current Published/Ledger/Run CAS.
    Commit(crate::AgentActionStoreFailure),
}
impl fmt::Display for HttpCorrectionEditFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("current HTTP correction could not be prepared")
    }
}
impl Error for HttpCorrectionEditFailure {}
