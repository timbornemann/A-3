//! Composition of the typed research aggregate with actual source delivery.
use super::{AskResearchWorkingSet, research_model};
use a3_application::{
    AskResearchDecision, ResearchEvidenceWindow, ResearchWorkUpdate, admit_research_work,
};
use a3_domain::{AskResearchSourceId, FileRevision, ResearchWorkState, SourceRange};

pub(super) struct WorkGuard {
    objective: String,
    previous: Option<ResearchWorkState>,
    required_revisions: Vec<FileRevision>,
    aliases: Vec<(u16, FileRevision)>,
    windows: Vec<OwnedWindow>,
    complete_originals: bool,
}

struct OwnedWindow {
    anchor: Option<a3_application::ResearchEvidenceAnchorId>,
    ordinal: u16,
    source_id: AskResearchSourceId,
    revision: FileRevision,
    range: SourceRange,
    text: String,
}

const CORE_PLAN_OUTCOMES: [&str; 3] = [
    "Explain the current entry points, APIs and integration constraints needed for the original request using the named original files. Do not require unrequested conventions or a pre-existing implementation of the requested new feature.",
    "Define the concrete requested implementation, interfaces, ordering and error handling as future design. Cover the original request and state safe reversible assumptions; do not search for the new implementation.",
    "Define concrete acceptance and regression tests for every requested outcome, success and failure case. New tests and test scaffolding are a design choice, not missing repository evidence.",
];

/// Literal request clauses, not inferred facts or a new semantic planner. Only split
/// outside code/quotes and after sentence/list introductions; keep file lists together.
fn request_clauses(objective: &str) -> Vec<String> {
    if objective.len() > 512 {
        return Vec::new();
    }
    let mut clauses = Vec::new();
    let mut start = 0;
    let mut quoted = None;
    let mut in_list = false;
    for (offset, ch) in objective.char_indices() {
        if matches!(ch, '`' | '"' | '\'') {
            if quoted == Some(ch) {
                quoted = None;
            } else if quoted.is_none() {
                quoted = Some(ch);
            }
        }
        if quoted.is_some() {
            continue;
        }
        let rest = &objective[offset + ch.len_utf8()..];
        let sentence = matches!(ch, '.' | '?' | ':' | ';') && rest.starts_with(char::is_whitespace);
        if sentence || ch == '\n' || (ch == ',' && in_list) {
            let end = offset + ch.len_utf8();
            let fragment = objective[start..end].trim();
            if !fragment.is_empty() {
                clauses.push(fragment.to_owned());
            }
            start = end;
            in_list = true;
            if clauses.len() == 5 {
                break;
            }
        }
    }
    if !objective[start..].trim().is_empty() {
        clauses.push(objective[start..].trim().to_owned());
    }
    // Keep at most six Core obligations under the unchanged twelve-call outer profile.
    // Adjacent tail clauses remain together; no original text or outcome disappears.
    clauses
}

pub(super) fn core_plan_contract(work: &ResearchWorkState) -> bool {
    work.questions().len() == 3
        && work
            .questions()
            .iter()
            .zip(CORE_PLAN_OUTCOMES)
            .enumerate()
            .all(|(index, (q, text))| {
                q.definition().outcome == text
                    && q.definition().priority == a3_domain::ResearchQuestionPriority::Required
                    && q.definition().kind
                        == if index == 0 {
                            a3_domain::ResearchQuestionKind::Repository
                        } else {
                            a3_domain::ResearchQuestionKind::Design
                        }
                    && q.definition()
                        .dependencies
                        .iter()
                        .map(|id| usize::from(id.get()))
                        .eq(1..=index)
            })
}
impl WorkGuard {
    /// Concrete repair guidance from actual delivery, not an inferred fact or a new read.
    pub(super) fn coverage_repair_hint(&self) -> Option<String> {
        use a3_application::ResearchOutputPhase;
        let phase = self.output_phase();
        let (ResearchOutputPhase::Analyze(id) | ResearchOutputPhase::SummarizeOriginals(id)) =
            phase
        else {
            return None;
        };
        let work = self.previous.as_ref()?;
        let active = work.question(id)?;
        let paths = super::query_path_candidates(&active.definition().outcome);
        let inventory = core_plan_contract(work) && id == a3_domain::ResearchQuestionId::FIRST;
        let last_required = work
            .questions()
            .iter()
            .filter(|q| {
                q.definition().priority == a3_domain::ResearchQuestionPriority::Required
                    && !q.resolved()
            })
            .count()
            == 1;
        let mut groups = Vec::new();
        for revision in &self.required_revisions {
            let named = paths.iter().any(|path| {
                let normalized = path.replace('\\', "/");
                super::index_path_matches_request(
                    &super::model_safe_path(revision.path()),
                    &normalized,
                    &format!("/{}", normalized.to_lowercase()),
                )
            });
            let linked = work
                .questions()
                .iter()
                .filter(|q| q.resolved())
                .filter_map(|q| q.result())
                .flat_map(|r| r.sources())
                .any(|s| &s.revision == revision);
            if !(inventory || named || last_required && !linked) {
                continue;
            }
            let mut anchors = self
                .windows
                .iter()
                .filter(|w| &w.revision == revision)
                .filter_map(|w| w.anchor.map(|a| a.get()))
                .collect::<Vec<_>>();
            anchors.sort_unstable();
            anchors.dedup();
            // Missing originals are not repairable by inventing a current anchor.
            if anchors.is_empty() || groups.len() == 8 {
                return None;
            }
            groups.push(format!(
                "[{}]",
                anchors
                    .iter()
                    .map(|n| format!("E{n}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        if groups.is_empty() {
            return None;
        }
        Some(format!(
            "Original coverage repair for Q{}. Return schema_version=5, work.questions=[], one result question_id={}, kind=interpretation, and decision kind=progress with note. Explain the requested behavior of EACH required original and cite at least one current anchor_ref from EVERY file group: {}. Multiple anchors in one group belong to the same file. Use only evidence supporting the explanation, not S labels or copied quotes. Do not substitute one file for another, omit a required file, propose new design or invent facts. The Core still validates every source; no extra read or repair is granted.",
            id.get(),
            id.get(),
            groups.join(" ")
        ))
    }

    pub(super) fn output_phase(&self) -> a3_application::ResearchOutputPhase {
        use a3_application::ResearchOutputPhase;
        match &self.previous {
            None => ResearchOutputPhase::Initialize,
            Some(state) if state.ready_to_finish() => ResearchOutputPhase::Finalize,
            Some(state) => state
                .next_question()
                .map(|id| {
                    if state.question(id).is_some_and(|q| {
                        q.definition().kind == a3_domain::ResearchQuestionKind::Design
                    }) {
                        ResearchOutputPhase::Design(id)
                    } else if self.complete_originals
                        && id == a3_domain::ResearchQuestionId::FIRST
                        && core_plan_contract(state)
                    {
                        ResearchOutputPhase::SummarizeOriginals(id)
                    } else {
                        ResearchOutputPhase::Analyze(id)
                    }
                })
                .unwrap_or(ResearchOutputPhase::Finalize),
        }
    }
    pub(super) fn new(objective: &str, state: &AskResearchWorkingSet) -> Self {
        Self {
            objective: objective.to_owned(),
            previous: state.work.clone(),
            required_revisions: state.work_required_revisions.clone(),
            complete_originals: state.complete_required_originals_delivered(),
            aliases: state
                .sources
                .iter()
                .filter_map(|s| {
                    u16::try_from(s.ordinal())
                        .ok()
                        .map(|id| (id, s.revision().clone()))
                })
                .collect(),
            windows: state
                .work_evidence_windows()
                .into_iter()
                .map(|w| OwnedWindow {
                    anchor: w.anchor,
                    ordinal: w.ordinal,
                    source_id: w.source_id,
                    revision: w.revision.clone(),
                    range: w.range,
                    text: w.text.to_owned(),
                })
                .collect(),
        }
    }
    pub(super) fn admit(
        &self,
        update: &ResearchWorkUpdate,
    ) -> Result<ResearchWorkState, research_model::DecisionIssue> {
        if (self.previous.is_none() && !update.results.is_empty())
            || (self.previous.is_some() && !update.questions.is_empty())
            || (self.output_phase() == a3_application::ResearchOutputPhase::Finalize
                && !update.results.is_empty())
        {
            return Err(research_model::DecisionIssue::WorkEvidence);
        }
        if matches!(
            self.output_phase(),
            a3_application::ResearchOutputPhase::SummarizeOriginals(_)
        ) && update.results.len() != 1
        {
            return Err(research_model::DecisionIssue::WorkEvidence);
        }
        if let a3_application::ResearchOutputPhase::Analyze(question)
        | a3_application::ResearchOutputPhase::SummarizeOriginals(question)
        | a3_application::ResearchOutputPhase::Design(question) = self.output_phase()
            && (update.results.len() > 1
                || update
                    .results
                    .iter()
                    .any(|result| result.question_id != question))
        {
            return Err(research_model::DecisionIssue::WorkEvidence);
        }
        let windows = self
            .windows
            .iter()
            .flat_map(|w| {
                // Aliases of one exact file revision may share delivered original bytes.
                // The retained source ID and range always belong to the actual delivery.
                let mut ordinals = vec![w.ordinal];
                for (ordinal, revision) in &self.aliases {
                    if revision == &w.revision
                        && !ordinals.contains(ordinal)
                        && update
                            .results
                            .iter()
                            .flat_map(|r| &r.evidence)
                            .any(|e| e.source_ordinal == *ordinal)
                    {
                        ordinals.push(*ordinal);
                    }
                }
                ordinals
                    .into_iter()
                    .map(move |ordinal| ResearchEvidenceWindow {
                        anchor: w.anchor,
                        ordinal,
                        source_id: w.source_id,
                        revision: &w.revision,
                        range: w.range,
                        text: &w.text,
                    })
            })
            .collect::<Vec<_>>();
        let state = admit_research_work(&self.objective, self.previous.as_ref(), update, &windows)
            .map_err(research_model::DecisionIssue::WorkAdmission)?;
        if self.previous.is_none()
            && !self.required_revisions.is_empty()
            && (!update.questions.iter().any(|q| {
                q.kind == a3_domain::ResearchQuestionKind::Repository
                    && q.priority == a3_domain::ResearchQuestionPriority::Required
            }) || !update.questions.iter().any(|q| {
                q.kind == a3_domain::ResearchQuestionKind::Design
                    && q.priority == a3_domain::ResearchQuestionPriority::Required
            }))
        {
            // Only a fully validated proposal can reach this Core-owned fallback. It
            // preserves the exact user goal, not the model's unsuitable classification.
            let clauses = request_clauses(&self.objective);
            if !clauses.is_empty() {
                return ResearchWorkState::new(
                    self.objective.clone(),
                    clauses
                        .into_iter()
                        .map(|fragment| a3_domain::ResearchQuestionDraft {
                            outcome: fragment.clone(),
                            request_fragment: fragment,
                            priority: a3_domain::ResearchQuestionPriority::Required,
                            kind: a3_domain::ResearchQuestionKind::Repository,
                            dependencies: vec![],
                        })
                        .collect(),
                )
                .map_err(|_| research_model::DecisionIssue::WorkEvidence);
            }
            return ResearchWorkState::new(self.objective.clone(), vec![a3_domain::ResearchQuestionDraft {
                request_fragment: super::utf8_prefix(&self.objective, 2048).to_owned(),
                outcome: if self.objective.len() <= 512 {self.objective.clone()} else {"Answer EVERY part of the complete original user question using the named original files, including all requested paths, destinations, conditions and ordering. Distinguish existing behavior from future proposals; never require a proposed new implementation to exist.".to_owned()},
                priority: a3_domain::ResearchQuestionPriority::Required,
                kind: a3_domain::ResearchQuestionKind::Repository,
                dependencies: vec![],
            }]).map_err(|_| research_model::DecisionIssue::WorkEvidence);
        }
        if core_plan_contract(&state)
            && update.results.iter().any(|r| {
                r.question_id.get() > 1 && r.kind != a3_domain::ResearchResultKind::DesignDecision
            })
        {
            return Err(research_model::DecisionIssue::WorkEvidence);
        }
        for proposal in &update.results {
            let question = state
                .question(proposal.question_id)
                .ok_or(research_model::DecisionIssue::WorkEvidence)?;
            if question.definition().kind == a3_domain::ResearchQuestionKind::Design {
                // A proposed file change is not a claim about existing code in that file.
                // Required original coverage remains enforced across the whole contract.
                continue;
            }
            for requested in super::query_path_candidates(&question.definition().outcome) {
                let normalized = requested.replace('\\', "/");
                let suffix = format!("/{}", normalized.to_lowercase());
                let matches = self
                    .required_revisions
                    .iter()
                    .filter(|revision| {
                        super::index_path_matches_request(
                            &super::model_safe_path(revision.path()),
                            &normalized,
                            &suffix,
                        )
                    })
                    .collect::<Vec<_>>();
                if let [revision] = matches.as_slice()
                    && !question
                        .result()
                        .is_some_and(|r| r.sources().iter().any(|s| &s.revision == *revision))
                {
                    return Err(research_model::DecisionIssue::WorkCoverage);
                }
            }
        }
        // The fixed plan inventory is the last source-bearing phase. Missing named
        // originals must be repaired here, not discovered after evidence-free design.
        let inventory_completed = core_plan_contract(&state)
            && update
                .results
                .iter()
                .any(|r| r.question_id == a3_domain::ResearchQuestionId::FIRST);
        if (state.ready_to_finish() || inventory_completed)
            && self.required_revisions.iter().any(|revision| {
                !state
                    .questions()
                    .iter()
                    .filter(|q| q.resolved())
                    .filter_map(|q| q.result())
                    .flat_map(|r| r.sources())
                    .any(|s| &s.revision == revision)
            })
        {
            return Err(research_model::DecisionIssue::WorkCoverage);
        }
        Ok(state)
    }
}

impl AskResearchWorkingSet {
    /// Cache ownership is not delivery: every required revision must be complete and
    /// its entire nonempty read range must occur in this exact emitted packet.
    fn complete_required_originals_delivered(&self) -> bool {
        let origin = a3_domain::SourcePosition::new(0, 0);
        let mut current = Vec::new();
        for window in self
            .work_evidence_windows()
            .into_iter()
            .filter(|w| w.anchor.is_some())
        {
            super::research_context::cover(
                &mut current,
                super::research_context::CoveredRange {
                    revision: window.revision.clone(),
                    start: window.range.start_position(),
                    end: window.range.end_position(),
                },
            );
        }
        !self.work_required_revisions.is_empty()
            && self.work_required_revisions.iter().all(|revision| {
                self.complete_files.contains(revision)
                    && self.read_coverage.iter().any(|read| {
                        &read.revision == revision
                            && read.start == origin
                            && read.end > origin
                            && current.iter().any(|delivered| {
                                delivered.revision == read.revision
                                    && delivered.start == origin
                                    && delivered.end >= read.end
                            })
                    })
            })
    }
    /// Lossless plan presentation: formatting cannot choose different error policies,
    /// interfaces or verification semantics from the already admitted design.
    pub(super) fn core_plan_answer(&self) -> Option<(String, Vec<u16>)> {
        let work = self.work.as_ref()?;
        if !work.ready_to_finish() || !core_plan_contract(work) {
            return None;
        }
        let changes = work.questions().get(1)?.result()?;
        let tests = work.questions().get(2)?.result()?;
        if changes.kind() != a3_domain::ResearchResultKind::DesignDecision
            || tests.kind() != a3_domain::ResearchResultKind::DesignDecision
        {
            return None;
        }
        let line = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let (basis, refs) = self.render_work_answer(false)?;
        Some((
            format!(
                "PLAN:\n\n## Summary\n\nDer Plan übernimmt die festgehaltenen Änderungs- und Testentscheidungen. Er ist noch nicht umgesetzt.\n\n## Implementation Changes\n\n1. {}\n\n## Interfaces\n\nSchnittstellen, Reihenfolge und Fehlerverhalten sind im vollständigen Änderungsentwurf oben festgelegt. Bestehende Integrationsgrenzen stehen in der Recherchegrundlage.\n\n## Test Plan\n\n1. {}\n\n## Assumptions\n\nEs gelten die ausdrücklich im Änderungsentwurf genannten Annahmen; diese Darstellung ergänzt keine weiteren Entscheidungen. Recherche ist keine Implementierungsverifikation.\n\n## Recherchegrundlage\n\n{basis}",
                line(changes.text()),
                line(tests.text())
            ),
            refs,
        ))
    }
    /// Planning has three Core-owned obligations. A speculative search for existing NEW
    /// behavior or test conventions cannot be promoted to an unrelated required blocker.
    pub(super) fn initialize_plan_work(
        &mut self,
        objective: &str,
    ) -> Result<(), a3_domain::ResearchWorkError> {
        use a3_domain::{
            ResearchQuestionDraft, ResearchQuestionId, ResearchQuestionKind,
            ResearchQuestionPriority,
        };
        if self.work.is_some() {
            return Ok(());
        }
        let fragment = super::utf8_prefix(objective, 2048).to_owned();
        self.work = Some(ResearchWorkState::new(
            objective.to_owned(),
            [
                (
                    CORE_PLAN_OUTCOMES[0],
                    ResearchQuestionKind::Repository,
                    vec![],
                ),
                (
                    CORE_PLAN_OUTCOMES[1],
                    ResearchQuestionKind::Design,
                    vec![ResearchQuestionId::FIRST],
                ),
                (
                    CORE_PLAN_OUTCOMES[2],
                    ResearchQuestionKind::Design,
                    vec![ResearchQuestionId::FIRST, ResearchQuestionId::new(2)?],
                ),
            ]
            .into_iter()
            .map(|(outcome, kind, dependencies)| ResearchQuestionDraft {
                request_fragment: fragment.clone(),
                outcome: outcome.to_owned(),
                priority: ResearchQuestionPriority::Required,
                kind,
                dependencies,
            })
            .collect(),
        )?);
        Ok(())
    }
    /// Deterministic cached callable selection for natural-language questions lacking identifiers.
    /// Small original units in explicitly requested files precede long bodies, which keep paging.
    pub(super) fn focus_work_cache(&mut self, published: &a3_domain::PublishedIndex) {
        if !self
            .work
            .as_ref()
            .and_then(|work| work.next_question().and_then(|id| work.question(id)))
            .is_some_and(|question| question.attempts().is_empty())
        {
            return;
        }
        let mut symbols = published
            .publication()
            .graph()
            .symbols()
            .iter()
            .take(32_768)
            .filter(|s| {
                self.work_required_revisions.contains(s.revision())
                    && matches!(
                        s.parsed().kind(),
                        a3_domain::SymbolKind::Function | a3_domain::SymbolKind::Method
                    )
            })
            .collect::<Vec<_>>();
        symbols.sort_by_key(|s| {
            let range = s.parsed().declaration_range();
            (
                range.end_byte().saturating_sub(range.start_byte()),
                s.revision().path(),
                range.start_byte(),
            )
        });
        let mut hint = self.active_work_hint().unwrap_or_default();
        let mut bytes = 0usize;
        for symbol in symbols.into_iter().take(16) {
            let range = symbol.parsed().declaration_range();
            let size = (range.end_byte().saturating_sub(range.start_byte())) as usize;
            if bytes.saturating_add(size) > self.evidence_limit / 2 {
                continue;
            }
            bytes += size;
            hint.push(' ');
            hint.push_str(symbol.parsed().name().as_str());
        }
        self.focus_hint(published, &hint);
    }
    pub(super) fn restore_work(
        &mut self,
        previous: &ResearchWorkState,
        mapping: &[(AskResearchSourceId, AskResearchSourceId)],
    ) -> Result<(), super::AgentSessionManagerFailure> {
        use a3_domain::{ResearchQuestionCheckpoint, ResearchQuestionStatus};
        let mut checkpoints = Vec::new();
        for question in previous.questions() {
            let mut status = question.status();
            let result = question
                .result()
                .map(|r| {
                    let sources = r
                        .sources()
                        .iter()
                        .map(|s| {
                            let mut source = s.clone();
                            if let Some((_, id)) = mapping.iter().find(|(id, _)| *id == s.source_id)
                            {
                                source.source_id = *id;
                            } else {
                                status = ResearchQuestionStatus::Stale;
                            }
                            source
                        })
                        .collect();
                    a3_domain::ResearchResult::new(
                        r.kind(),
                        r.text().to_owned(),
                        sources,
                        r.boundary(),
                    )
                })
                .transpose()
                .map_err(|_| super::AgentSessionManagerFailure::InvalidOutput)?;
            if question.definition().dependencies.iter().any(|id| {
                checkpoints.get(usize::from(id.get() - 1)).is_some_and(
                    |q: &ResearchQuestionCheckpoint| {
                        !matches!(
                            q.status,
                            ResearchQuestionStatus::Answered | ResearchQuestionStatus::Limited
                        )
                    },
                )
            }) {
                status = ResearchQuestionStatus::Stale;
            }
            if status == ResearchQuestionStatus::Blocked {
                status = if result.is_some() {
                    ResearchQuestionStatus::Stale
                } else if question.attempts().is_empty() {
                    ResearchQuestionStatus::Open
                } else {
                    ResearchQuestionStatus::Active
                };
            }
            // Explicit continuation may try a new frontier, never erase same-packet history.
            let attempts = if status == ResearchQuestionStatus::Stale {
                Default::default()
            } else {
                question.attempts().clone()
            };
            checkpoints.push(ResearchQuestionCheckpoint {
                definition: question.definition().clone(),
                status,
                result,
                attempts,
                exclusions: if status == ResearchQuestionStatus::Stale {
                    Default::default()
                } else {
                    question.exclusions().clone()
                },
            });
        }
        self.work = Some(
            ResearchWorkState::restore(
                previous.objective().to_owned(),
                previous.revision().saturating_add(1),
                checkpoints,
            )
            .and_then(|state| state.with_restored_accesses(previous.accesses().to_vec()))
            .map_err(|_| super::AgentSessionManagerFailure::InvalidOutput)?,
        );
        Ok(())
    }
    /// Same source bytes and semantic state, independent of source numbering/order or notes.
    pub(super) fn work_packet_key(&self) -> a3_domain::ContentHash {
        let mut windows = self
            .work_evidence_windows()
            .into_iter()
            .map(|w| {
                (
                    w.revision.path().as_bytes().to_vec(),
                    *w.revision.content_hash().as_bytes(),
                    w.range.start_byte(),
                    w.range.end_byte(),
                )
            })
            .collect::<Vec<_>>();
        windows.sort();
        windows.dedup();
        let mut digest = blake3::Hasher::new();
        digest.update(b"a3.research-analysis-packet.v1\0");
        for (path, hash, start, end) in windows {
            digest.update(&(path.len() as u64).to_le_bytes());
            digest.update(&path);
            digest.update(&hash);
            digest.update(&start.to_le_bytes());
            digest.update(&end.to_le_bytes());
        }
        // A newly admitted prerequisite changes the reasoning input even if source bytes match.
        if let Some(work) = &self.work {
            for question in work.questions().iter().filter(|q| q.resolved()) {
                digest.update(&question.id().get().to_le_bytes());
                if let Some(result) = question.result() {
                    digest.update(result.text().as_bytes());
                }
            }
        }
        a3_domain::ContentHash::from_bytes(*digest.finalize().as_bytes())
    }

    pub(super) fn active_work_hint(&self) -> Option<String> {
        let work = self.work.as_ref()?;
        let question = work.question(work.next_question()?)?;
        Some(format!(
            "{} {} {}",
            question.definition().outcome,
            question.definition().request_fragment,
            self.work_required_revisions
                .iter()
                .map(|r| super::model_safe_path(r.path()))
                .collect::<Vec<_>>()
                .join(" ")
        ))
    }

    pub(super) fn work_followup_note(&self) -> Option<a3_application::AskResearchDecisionNote> {
        let mut note = self.last_note.clone()?;
        note.gap = format!("{} {}", self.active_work_hint()?, note.gap);
        Some(note)
    }

    pub(super) fn work_context(&self) -> String {
        let full = self.render_work_context(false);
        let mut available = self.evidence_limit.saturating_sub(
            self.work
                .as_ref()
                .map_or(0, |work| work.objective().len())
                .saturating_add(self.work_packet_reserve()),
        );
        if self
            .work
            .as_ref()
            .and_then(|w| w.next_question().and_then(|id| w.question(id)))
            .is_some_and(|q| q.definition().kind == a3_domain::ResearchQuestionKind::Repository)
        {
            // Prefer the partitioned contract over spending original-source capacity
            // on inactive definitions and redundant per-file status prose.
            available = available.min(self.evidence_limit / 3);
        }
        if full.len() <= available {
            full
        } else {
            self.render_work_context(true)
        }
    }

    /// Repository claims need room for originals. A dependent design turn instead owns
    /// its frozen decisions; reserve only packet framing before optional source windows.
    pub(super) fn work_packet_reserve(&self) -> usize {
        if self
            .work
            .as_ref()
            .and_then(|w| w.next_question().and_then(|id| w.question(id)))
            .is_some_and(|q| q.definition().kind == a3_domain::ResearchQuestionKind::Design)
        {
            256
        } else {
            (self.evidence_limit / 3).clamp(512, 1536)
        }
    }

    // A partition changes only presentation, never the frozen question contract, selection,
    // evidence requirements, or the enclosing controller's counters/deadline.
    fn render_work_context(&self, compact: bool) -> String {
        let Some(work) = &self.work else {
            return String::new();
        };
        let mut text = String::from(if compact {
            "\nCORE RESEARCH CONTRACT (immutable):\n"
        } else {
            "\nCORE RESEARCH CONTRACT (immutable; results are source-bound interpretations, not verified facts):\n"
        });
        if !self.work_required_revisions.is_empty() && !compact {
            text.push_str("Required original file coverage across work.results (each named file needs an original E-window reference; a caller does not prove the callee body):\n");
            for revision in &self.work_required_revisions {
                let covered = work
                    .questions()
                    .iter()
                    .filter(|q| q.resolved())
                    .filter_map(|q| q.result())
                    .flat_map(|r| r.sources())
                    .any(|s| &s.revision == revision);
                text.push_str(&format!(
                    "{}: {}\n",
                    super::model_safe_path(revision.path()),
                    if covered {
                        "original evidence linked"
                    } else {
                        "original evidence still required"
                    }
                ));
            }
        }
        for question in work.questions() {
            if compact {
                text.push_str(&format!(
                    "Q{} {:?} {:?}\n",
                    question.id().get(),
                    question.definition().priority,
                    question.status()
                ));
            } else {
                text.push_str(&format!(
                    "Q{} {:?} {:?}: {}\n",
                    question.id().get(),
                    question.definition().priority,
                    question.status(),
                    question.definition().outcome
                ));
            }
        }
        if compact {
            text.push_str("Partitioned view; inactive definitions unchanged.\n");
        }
        if let Some(id) = work.next_question().and_then(|id| work.question(id)) {
            text.push_str(&format!(
                "ACTIVE Q{}: {}\n",
                id.id().get(),
                id.definition().outcome
            ));
            match id.definition().kind {
                a3_domain::ResearchQuestionKind::Repository if compact => text.push_str("Core question kind=repository; use current E evidence.\n"),
                a3_domain::ResearchQuestionKind::Design if compact => text.push_str("Core question kind=design; designDecision, evidence=[]. New implementation need not exist. Prior DesignDecision policies bind tests.\n"),
                a3_domain::ResearchQuestionKind::Repository => text.push_str("Core question kind=repository. Answer existing behavior as an interpretation using current original evidence; do not repeat answered questions.\n"),
                a3_domain::ResearchQuestionKind::Design => text.push_str("Core question kind=design. Propose the requested NEW behavior as designDecision with evidence=[]. Its implementation need not exist. Use admitted existing constraints and reversible assumptions. Only prerequisite DesignDecision results below bind future policies; derive tests from them without stronger guarantees or changed policies. Do not search for the new feature or repeat answered questions.\n"),
            }
        } else if work.ready_to_finish() {
            text.push_str("ALL REQUIRED QUESTIONS RESOLVED. Format the result; optional investigation must not restart research.\n");
        }
        let active = work.next_question().and_then(|id| work.question(id));
        let results = work
            .questions()
            .iter()
            .filter(|q| {
                q.resolved()
                    && active
                        .is_none_or(|current| current.definition().dependencies.contains(&q.id()))
            })
            .collect::<Vec<_>>();
        let preview_limit = if compact {
            self.evidence_limit
                .saturating_sub(
                    work.objective()
                        .len()
                        .saturating_add(self.work_packet_reserve())
                        .saturating_add(text.len()),
                )
                .checked_div(results.len().max(1))
                .unwrap_or(0)
                .saturating_sub(80)
                .min(384)
        } else {
            384
        };
        for question in results {
            if let Some(result) = question.result() {
                let references = result
                    .sources()
                    .iter()
                    .filter_map(|e| self.sources.iter().find(|s| s.id() == e.source_id))
                    .map(|s| format!("S{}", s.ordinal()))
                    .collect::<Vec<_>>()
                    .join(",");
                text.push_str(&format!(
                    "Q{} result: kind={:?}; {} [{}]\n",
                    question.id().get(),
                    result.kind(),
                    if result.kind() == a3_domain::ResearchResultKind::DesignDecision {
                        // Dependent verification must see the actual frozen decisions,
                        // especially policies near the end, not just a 384-byte synopsis.
                        result.text()
                    } else {
                        super::utf8_prefix(result.text(), preview_limit)
                    },
                    references
                ));
            }
        }
        text
    }

    pub(super) fn accept_work(
        &mut self,
        query: &str,
        decision: &AskResearchDecision,
        analysis_receipt: Option<(a3_domain::ResearchQuestionId, a3_domain::ContentHash)>,
    ) -> Result<(), super::AgentSessionManagerFailure> {
        let note = match decision {
            AskResearchDecision::Answer { note, .. }
            | AskResearchDecision::Research { note, .. } => note,
        };
        if let Some(update) = &note.work {
            let mut guard = WorkGuard::new(query, self);
            if let Some((id, packet)) = analysis_receipt {
                guard
                    .previous
                    .as_mut()
                    .ok_or(super::AgentSessionManagerFailure::InvalidOutput)?
                    .begin_analysis(id, packet)
                    .map_err(|_| super::AgentSessionManagerFailure::InvalidOutput)?;
            }
            // Packet acknowledgement and result admission either both succeed or change nothing.
            let admitted = guard
                .admit(update)
                .map_err(|_| super::AgentSessionManagerFailure::InvalidOutput)?;
            self.work = Some(admitted);
        }
        Ok(())
    }

    pub(super) fn work_answer(&self) -> Option<(String, Vec<u16>)> {
        self.render_work_answer(true)
    }

    fn render_work_answer(&self, include_design: bool) -> Option<(String, Vec<u16>)> {
        let work = self.work.as_ref()?;
        let mut answer = String::new();
        let mut ordinals = Vec::new();
        for question in work.questions().iter().filter(|q| {
            q.definition().priority == a3_domain::ResearchQuestionPriority::Required
                && q.resolved()
                && (include_design
                    || q.definition().kind != a3_domain::ResearchQuestionKind::Design)
        }) {
            let result = question.result()?;
            answer.push_str(&format!(
                "### {}\n\n{}",
                question.definition().outcome,
                result.text()
            ));
            for reference in result.sources() {
                let source = self
                    .sources
                    .iter()
                    .find(|source| source.id() == reference.source_id)?;
                let ordinal = u16::try_from(source.ordinal()).ok()?;
                answer.push_str(&format!(" 【S{ordinal}】"));
                if !ordinals.contains(&ordinal) {
                    ordinals.push(ordinal);
                }
            }
            answer.push_str("\n\n");
        }
        (!answer.is_empty()).then_some((answer, ordinals))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_application::{ResearchOutputPhase, ResearchQuoteProposal, ResearchResultProposal};
    use a3_domain::{
        ContentHash, RepositoryPath, ResearchQuestionDraft, ResearchQuestionId,
        ResearchQuestionKind, ResearchQuestionPriority, ResearchResultKind, SourcePosition,
    };
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn research_plan_inventory_requires_complete_current_original_delivery() -> TestResult {
        let mut state = AskResearchWorkingSet::new(4096);
        state.initialize_plan_work("Plan audit.py changes")?;
        let original = guard(None)?.windows.remove(0);
        let revision = original.revision.clone();
        let source = a3_application::AskResearchSource::new(
            a3_domain::AgentSessionId::from_bytes([3; 32]),
            a3_domain::AgentSessionSequence::FIRST,
            original.source_id,
            1,
            revision.clone(),
            Some(original.range),
            None,
            a3_domain::AskResearchSourceKind::File,
            a3_domain::AskResearchSelectionReason::ExactNameOrPath,
        )?;
        state.record_read_coverage(&source, 1, &original.text);
        state.sources.push(source);
        state.excerpts.push(super::super::ResearchSourceExcerpt {
            ordinal: 1,
            path: "audit.py".to_owned(),
            start_line: 1,
            text: original.text,
        });
        state.work_required_revisions.push(revision.clone());
        let analyze = ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST);
        let summary = ResearchOutputPhase::SummarizeOriginals(ResearchQuestionId::FIRST);
        assert_eq!(
            WorkGuard::new("Plan audit.py changes", &state).output_phase(),
            analyze
        );
        state.complete_files.push(revision.clone());
        assert!(
            !state.complete_required_originals_delivered(),
            "cached is not delivered"
        );
        let packet = state.model_evidence("Plan audit.py changes", &[]);
        assert!(packet.contains("audit_log.txt"));
        assert_eq!(
            WorkGuard::new("Plan audit.py changes", &state).output_phase(),
            summary
        );
        let empty = ResearchWorkUpdate {
            questions: vec![],
            results: vec![],
        };
        assert!(
            WorkGuard::new("Plan audit.py changes", &state)
                .admit(&empty)
                .is_err()
        );
        state.read_coverage[0].end = SourcePosition::new(1, 0);
        assert!(
            !state.complete_required_originals_delivered(),
            "partial packet cannot close a full file"
        );
        state.read_coverage[0].end = original.range.end_position();
        state.complete_files[0] =
            FileRevision::new(revision.path().clone(), ContentHash::from_bytes([9; 32]));
        assert!(
            !state.complete_required_originals_delivered(),
            "different hash is not complete"
        );
        state.complete_files[0] = revision;
        state.work = Some(ResearchWorkState::new(
            "audit destination".to_owned(),
            vec![draft()],
        )?);
        assert_eq!(
            WorkGuard::new("audit destination", &state).output_phase(),
            analyze,
            "general Ask stays nullable"
        );
        state.current_source_delivery.clear();
        assert!(
            !state.complete_required_originals_delivered(),
            "past delivery is not current evidence"
        );
        Ok(())
    }

    #[test]
    fn research_repair_hints_follow_the_same_phase_evidence_contract() {
        use a3_application::ResearchOutputPhase;
        let issue = research_model::DecisionIssue::WorkEvidence;
        let design = issue.repair_hint_for_phase(
            Some(ResearchOutputPhase::Design(
                a3_domain::ResearchQuestionId::FIRST,
            )),
            4,
        );
        assert!(design.contains("kind=designDecision, evidence=[]"));
        assert!(!design.contains("E-window anchor_ref"));
        let analysis = issue.repair_hint_for_phase(
            Some(ResearchOutputPhase::Analyze(
                a3_domain::ResearchQuestionId::FIRST,
            )),
            4,
        );
        assert!(analysis.contains("kind=interpretation"));
        assert!(analysis.contains("E-window anchor_ref"));
        assert!(design.len() <= 768 && analysis.len() <= 768);
        let summary = issue.repair_hint_for_phase(
            Some(ResearchOutputPhase::SummarizeOriginals(
                ResearchQuestionId::FIRST,
            )),
            4,
        );
        assert!(summary.contains("exactly one result"));
        assert!(!summary.contains("return results=[]"));
        assert!(summary.len() <= 768);
        assert_eq!(issue.repair_hint_for_phase(None, 4), issue.repair_hint(4));
    }

    #[test]
    fn research_literal_clauses_preserve_source_lists_quotes_and_exact_tail() -> TestResult {
        for objective in [
            "Inspect a.py, b.py and c.py: callers, writer, destination, cwd, mode, errors, tests.",
            "Inspect `call(a, b)` and \"a. b\". Explain paths;\n  explain order; explain mode;\n explain errors;\n\n explain tests; preserve  spaces.",
            "Erkläre main.py, taskflow/manager.py und storage.py. Nenne Default, Dateiname und Fehler.",
            "Untersuche Änderungen: gültig, ungültig, Unicode, Äpfel, Böden, Größe, Öl.",
        ] {
            let clauses = request_clauses(objective);
            assert!(!clauses.is_empty() && clauses.len() <= 6);
            let mut remaining = objective;
            for clause in &clauses {
                assert!(remaining.trim_start().starts_with(clause));
                remaining = remaining
                    .trim_start()
                    .strip_prefix(clause)
                    .ok_or("literal clause")?;
            }
            assert!(remaining.trim().is_empty());
            let work = ResearchWorkState::new(
                objective.to_owned(),
                clauses
                    .into_iter()
                    .map(|fragment| a3_domain::ResearchQuestionDraft {
                        outcome: fragment.clone(),
                        request_fragment: fragment,
                        priority: a3_domain::ResearchQuestionPriority::Required,
                        kind: a3_domain::ResearchQuestionKind::Repository,
                        dependencies: vec![],
                    })
                    .collect(),
            )?;
            assert_eq!(work.objective(), objective);
        }
        assert_eq!(
            request_clauses("Read a.py, b.py and `f(a, b)`. Explain."),
            vec!["Read a.py, b.py and `f(a, b)`.", "Explain."]
        );
        assert!(request_clauses(&"ü".repeat(257)).is_empty());
        Ok(())
    }

    fn draft() -> ResearchQuestionDraft {
        ResearchQuestionDraft {
            request_fragment: "audit destination".to_owned(),
            outcome: "Explain the audit destination".to_owned(),
            priority: ResearchQuestionPriority::Required,
            kind: ResearchQuestionKind::Repository,
            dependencies: vec![],
        }
    }

    #[test]
    fn research_core_plan_preserves_late_design_policy_context_and_ledger() -> TestResult {
        let mut state = AskResearchWorkingSet::new(4600);
        state.initialize_plan_work("Plan audit destination")?;
        let original = guard(None)?.windows.remove(0);
        state.sources.push(a3_application::AskResearchSource::new(
            a3_domain::AgentSessionId::from_bytes([3; 32]),
            a3_domain::AgentSessionSequence::FIRST,
            original.source_id,
            1,
            original.revision.clone(),
            None,
            None,
            a3_domain::AskResearchSourceKind::File,
            a3_domain::AskResearchSelectionReason::ExactNameOrPath,
        )?);
        state.work.as_mut().ok_or("work")?.resolve(
            ResearchQuestionId::FIRST,
            a3_domain::ResearchResult::new(
                ResearchResultKind::Interpretation,
                "Existing writer appends to the audit destination.".to_owned(),
                vec![a3_domain::ResearchResultSource {
                    source_id: original.source_id,
                    revision: original.revision,
                    range: original.range,
                }],
                None,
            )?,
        )?;
        let design = format!(
            "{} Stop on first error. Keep earlier writes; no rollback and no skipping.",
            "Keep the audit API unchanged. ".repeat(80)
        );
        state.work.as_mut().ok_or("work")?.resolve(
            ResearchQuestionId::new(2)?,
            a3_domain::ResearchResult::new(
                ResearchResultKind::DesignDecision,
                design.clone(),
                vec![],
                None,
            )?,
        )?;
        assert!(state.core_plan_answer().is_none());
        let packet = state.model_evidence("Plan audit destination", &[]);
        assert!(
            packet.contains(&design),
            "late policies are mandatory, never a preview"
        );
        assert!(packet.len() <= 4600);
        assert!(packet.contains("kind=Interpretation"));
        assert!(packet.contains("kind=DesignDecision"));
        assert_eq!(state.work_packet_reserve(), 256);
        let tests = "Verify first-error termination, retained earlier writes and no skipping.";
        state.work.as_mut().ok_or("work")?.resolve(
            ResearchQuestionId::new(3)?,
            a3_domain::ResearchResult::new(
                ResearchResultKind::DesignDecision,
                tests.to_owned(),
                vec![],
                None,
            )?,
        )?;
        let (plan, refs) = state.core_plan_answer().ok_or("core plan")?;
        let normalized_design = design.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(plan.contains(&normalized_design));
        assert_eq!(
            plan.matches(&normalized_design).count(),
            1,
            "the plan must not repeat its complete design in the source appendix"
        );
        assert!(plan.contains(tests));
        assert_eq!(refs, vec![1]);
        let ledger = a3_domain::AgentWorkPlan::from_reviewed_markdown(&plan)?;
        assert_eq!(ledger.steps().len(), 2);
        assert_eq!(state.core_plan_answer(), Some((plan, refs)));
        Ok(())
    }

    #[test]
    fn research_coverage_repair_groups_only_actual_required_original_windows() -> TestResult {
        let mut guard = guard(Some(ResearchWorkState::new(
            "audit destination".to_owned(),
            vec![draft()],
        )?))?;
        assert!(
            guard.coverage_repair_hint().is_none(),
            "legacy source aliases are not E windows"
        );
        guard.windows[0].anchor = Some(a3_application::ResearchEvidenceAnchorId::new(1)?);
        let hint = guard.coverage_repair_hint().ok_or("hint")?;
        assert!(hint.contains("[E1]"));
        assert!(hint.len() <= 768);
        assert!(
            !hint.contains("audit_log.txt"),
            "no original text in repair diagnostics"
        );
        let foreign = FileRevision::new(
            RepositoryPath::try_from_bytes(b"other.py".to_vec())?,
            ContentHash::from_bytes([4; 32]),
        );
        guard.windows.push(OwnedWindow {
            anchor: Some(a3_application::ResearchEvidenceAnchorId::new(2)?),
            ordinal: 2,
            source_id: AskResearchSourceId::from_bytes([4; 32]),
            revision: foreign.clone(),
            range: SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?,
            text: "mode".to_owned(),
        });
        assert!(!guard.coverage_repair_hint().ok_or("hint")?.contains("E2"));
        guard.required_revisions.push(foreign);
        assert!(
            guard
                .coverage_repair_hint()
                .ok_or("hint")?
                .contains("[E1] [E2]")
        );
        guard.windows[1].anchor = None;
        assert!(
            guard.coverage_repair_hint().is_none(),
            "missing originals cannot get invented anchors"
        );
        Ok(())
    }

    #[test]
    fn research_relative_named_original_is_required_in_its_own_question() -> TestResult {
        let mut definition = draft();
        definition.outcome = "Explain audit.py and taskflow/config.py.".to_owned();
        let previous =
            ResearchWorkState::new("audit destination".to_owned(), vec![definition, draft()])?;
        let mut guard = guard(Some(previous))?;
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"taskflow/config.py".to_vec())?,
            ContentHash::from_bytes([4; 32]),
        );
        guard.required_revisions.push(revision.clone());
        guard.windows[0].anchor = Some(a3_application::ResearchEvidenceAnchorId::new(1)?);
        guard.windows.push(OwnedWindow {
            anchor: Some(a3_application::ResearchEvidenceAnchorId::new(2)?),
            ordinal: 2,
            source_id: AskResearchSourceId::from_bytes([4; 32]),
            revision,
            range: SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?,
            text: "mode".to_owned(),
        });
        let mut result = answer()?;
        result.evidence.clear();
        result
            .anchors
            .push(a3_application::ResearchEvidenceAnchorId::new(1)?);
        let mut update = ResearchWorkUpdate {
            questions: vec![],
            results: vec![result],
        };
        assert!(
            matches!(
                guard.admit(&update),
                Err(research_model::DecisionIssue::WorkCoverage)
            ),
            "relative named paths need evidence now, not in an unrelated final question"
        );
        assert_eq!(
            guard.previous.as_ref().ok_or("previous")?.resolved_count(),
            0
        );
        update.results[0]
            .anchors
            .push(a3_application::ResearchEvidenceAnchorId::new(2)?);
        let admitted = guard.admit(&update).map_err(|e| format!("{e:?}"))?;
        assert_eq!(admitted.resolved_count(), 1);
        assert_eq!(
            admitted.questions()[0]
                .result()
                .ok_or("result")?
                .sources()
                .len(),
            2
        );
        Ok(())
    }

    #[test]
    fn research_core_plan_requires_named_originals_before_any_design() -> TestResult {
        let mut working = AskResearchWorkingSet::new(4096);
        working.initialize_plan_work("Plan changes in audit.py and config.py")?;
        let mut guard = guard(working.work)?;
        guard.objective = "Plan changes in audit.py and config.py".to_owned();
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"config.py".to_vec())?,
            ContentHash::from_bytes([4; 32]),
        );
        guard.required_revisions.push(revision.clone());
        let mut update = ResearchWorkUpdate {
            questions: vec![],
            results: vec![answer()?],
        };
        assert!(
            matches!(
                guard.admit(&update),
                Err(research_model::DecisionIssue::WorkCoverage)
            ),
            "reject incomplete inventory now, not after evidence-free design steps"
        );
        guard.windows.push(OwnedWindow {
            anchor: None,
            ordinal: 2,
            source_id: AskResearchSourceId::from_bytes([4; 32]),
            revision,
            range: SourceRange::new(0, 4, SourcePosition::new(0, 0), SourcePosition::new(0, 4))?,
            text: "mode".to_owned(),
        });
        update.results[0].evidence.push(ResearchQuoteProposal {
            source_ordinal: 2,
            quote: "mode".to_owned(),
        });
        let admitted = guard
            .admit(&update)
            .map_err(|_| "complete inventory rejected")?;
        assert_eq!(admitted.next_question(), Some(ResearchQuestionId::new(2)?));
        assert_eq!(
            admitted.questions()[0]
                .result()
                .ok_or("inventory")?
                .sources()
                .len(),
            2
        );
        Ok(())
    }

    #[test]
    fn research_design_kind_is_explicit_in_full_and_partitioned_context() -> TestResult {
        let mut definition = draft();
        definition.kind = ResearchQuestionKind::Design;
        let work = ResearchWorkState::new("audit destination".to_owned(), vec![definition])?;
        let mut state = AskResearchWorkingSet::new(4096);
        state.work = Some(work);
        for compact in [false, true] {
            let packet = state.render_work_context(compact);
            assert!(packet.contains("kind=design"));
            assert!(packet.contains("designDecision"));
            assert!(packet.contains("evidence=[]"));
            assert!(packet.contains("implementation need not exist"));
            assert!(!packet.contains("Resolve this question using current original evidence"));
        }
        Ok(())
    }

    #[test]
    fn research_design_file_target_does_not_need_a_fictitious_original_quote() -> TestResult {
        let mut design = draft();
        design.outcome = "Plan a new import feature in audit.py".to_owned();
        design.kind = ResearchQuestionKind::Design;
        design.dependencies = vec![ResearchQuestionId::FIRST];
        let mut state =
            ResearchWorkState::new("audit destination".to_owned(), vec![draft(), design])?;
        let source = guard(None)?.windows.remove(0);
        state.resolve(
            ResearchQuestionId::FIRST,
            a3_domain::ResearchResult::new(
                ResearchResultKind::Interpretation,
                "Existing audit destination observed.".to_owned(),
                vec![a3_domain::ResearchResultSource {
                    source_id: source.source_id,
                    revision: source.revision,
                    range: source.range,
                }],
                None,
            )?,
        )?;
        let guard = guard(Some(state))?;
        let state = guard
            .admit(&ResearchWorkUpdate {
                questions: vec![],
                results: vec![ResearchResultProposal {
                    question_id: ResearchQuestionId::new(2)?,
                    kind: ResearchResultKind::DesignDecision,
                    text: "Add the requested import path and regression tests as future changes."
                        .to_owned(),
                    evidence: vec![],
                    anchors: vec![],
                }],
            })
            .map_err(|e| format!("{e:?}"))?;
        assert!(state.ready_to_finish());
        Ok(())
    }
    fn guard(previous: Option<ResearchWorkState>) -> Result<WorkGuard, Box<dyn std::error::Error>> {
        let revision = FileRevision::new(
            RepositoryPath::try_from_bytes(b"audit.py".to_vec())?,
            ContentHash::from_bytes([1; 32]),
        );
        Ok(WorkGuard {
            objective: "audit destination".to_owned(),
            complete_originals: false,
            previous,
            required_revisions: vec![revision.clone()],
            aliases: vec![],
            windows: vec![OwnedWindow {
                anchor: None,
                ordinal: 1,
                source_id: AskResearchSourceId::from_bytes([2; 32]),
                revision,
                range: SourceRange::new(
                    0,
                    13,
                    SourcePosition::new(0, 0),
                    SourcePosition::new(0, 13),
                )?,
                text: "audit_log.txt".to_owned(),
            }],
        })
    }
    fn answer() -> Result<ResearchResultProposal, Box<dyn std::error::Error>> {
        Ok(ResearchResultProposal {
            anchors: vec![],
            question_id: ResearchQuestionId::new(1)?,
            kind: ResearchResultKind::Interpretation,
            text: "The default destination is audit_log.txt.".to_owned(),
            evidence: vec![ResearchQuoteProposal {
                source_ordinal: 1,
                quote: "audit_log.txt".to_owned(),
            }],
        })
    }
    #[test]
    fn research_phase_boundary_rejects_redefinitions_and_premature_results() -> TestResult {
        let initial = guard(None)?;
        assert_eq!(initial.output_phase(), ResearchOutputPhase::Initialize);
        let mut mistaken = draft();
        mistaken.kind = ResearchQuestionKind::Design;
        let fallback = initial
            .admit(&ResearchWorkUpdate {
                questions: vec![mistaken],
                results: vec![],
            })
            .map_err(|_| "fallback")?;
        assert_eq!(fallback.objective(), initial.objective);
        assert_eq!(
            fallback.questions()[0].definition().kind,
            ResearchQuestionKind::Repository
        );
        assert!(!fallback.ready_to_finish());
        assert!(fallback.questions()[0].result().is_none());
        assert!(
            initial
                .admit(&ResearchWorkUpdate {
                    questions: vec![],
                    results: vec![]
                })
                .is_err()
        );
        assert!(
            initial
                .admit(&ResearchWorkUpdate {
                    questions: vec![draft()],
                    results: vec![answer()?]
                })
                .is_err()
        );
        let state = initial
            .admit(&ResearchWorkUpdate {
                questions: vec![draft()],
                results: vec![],
            })
            .map_err(|_| "initial admission")?;
        let analyzing = guard(Some(state))?;
        assert_eq!(
            analyzing.output_phase(),
            ResearchOutputPhase::Analyze(ResearchQuestionId::FIRST)
        );
        assert!(
            analyzing
                .admit(&ResearchWorkUpdate {
                    questions: vec![draft()],
                    results: vec![]
                })
                .is_err()
        );
        let ready = analyzing
            .admit(&ResearchWorkUpdate {
                questions: vec![],
                results: vec![answer()?],
            })
            .map_err(|_| "answer admission")?;
        let finalizing = guard(Some(ready))?;
        assert_eq!(finalizing.output_phase(), ResearchOutputPhase::Finalize);
        assert!(
            finalizing
                .admit(&ResearchWorkUpdate {
                    questions: vec![],
                    results: vec![answer()?]
                })
                .is_err()
        );
        assert!(
            finalizing
                .admit(&ResearchWorkUpdate {
                    questions: vec![],
                    results: vec![]
                })
                .is_ok()
        );
        Ok(())
    }

    #[test]
    fn research_core_keeps_literal_obligations_but_preserves_mixed_and_existing_contracts()
    -> TestResult {
        let mut initial = guard(None)?;
        initial.objective = "Explain audit.py destination. Name the writer.".to_owned();
        let mut first = draft();
        first.request_fragment = "destination".to_owned();
        let mut summary = first.clone();
        summary.outcome = "Summarize the already investigated audit again".to_owned();
        let proposal = ResearchWorkUpdate {
            questions: vec![first.clone(), summary.clone()],
            results: vec![],
        };
        let core = initial.admit(&proposal).map_err(|_| "core obligations")?;
        assert_eq!(
            core.questions()
                .iter()
                .map(|q| q.definition().outcome.as_str())
                .collect::<Vec<_>>(),
            vec!["Explain audit.py destination.", "Name the writer."]
        );
        assert!(core.questions().iter().all(|q| q.result().is_none()));

        summary.kind = ResearchQuestionKind::Design;
        let mixed = ResearchWorkUpdate {
            questions: vec![first.clone(), summary],
            results: vec![],
        };
        let retained = initial.admit(&mixed).map_err(|_| "mixed contract")?;
        assert!(
            retained
                .questions()
                .iter()
                .map(|q| q.definition())
                .eq(&mixed.questions)
        );

        let old = ResearchWorkState::new(initial.objective.clone(), proposal.questions.clone())?;
        initial.previous = Some(old.clone());
        assert_eq!(
            initial
                .admit(&ResearchWorkUpdate {
                    questions: vec![],
                    results: vec![]
                })
                .map_err(|_| "old contract")?,
            old
        );
        initial.previous = None;
        initial.required_revisions.clear();
        let unnamed = initial.admit(&proposal).map_err(|_| "unnamed contract")?;
        assert!(
            unnamed
                .questions()
                .iter()
                .map(|q| q.definition())
                .eq(&proposal.questions)
        );
        Ok(())
    }
    #[test]
    fn research_all_named_files_need_original_evidence_before_closure() -> TestResult {
        let state = ResearchWorkState::new("audit destination".to_owned(), vec![draft()])?;
        let mut guard = guard(Some(state.clone()))?;
        guard.required_revisions.push(FileRevision::new(
            RepositoryPath::try_from_bytes(b"manager.py".to_vec())?,
            ContentHash::from_bytes([3; 32]),
        ));
        assert!(matches!(
            guard.admit(&ResearchWorkUpdate {
                questions: vec![],
                results: vec![answer()?]
            }),
            Err(research_model::DecisionIssue::WorkCoverage)
        ));
        assert_eq!(guard.previous, Some(state));
        Ok(())
    }
    #[test]
    fn research_packet_identity_does_not_depend_on_progress_counters() -> TestResult {
        let mut state = AskResearchWorkingSet::new(4096);
        let definition = ResearchWorkState::new("audit destination".to_owned(), vec![draft()])?;
        state.work = Some(definition);
        let first = state.work_packet_key();
        // Public wording cannot manufacture a new analysis packet.
        state.evidence_revision = 9;
        state.delivery_revision = 10;
        assert_eq!(first, state.work_packet_key());
        Ok(())
    }

    #[test]
    fn research_large_contract_partitions_without_dropping_questions_or_active_outcome()
    -> TestResult {
        let mut state = AskResearchWorkingSet::new(4096);
        let mut definitions = Vec::new();
        for n in 1..=32 {
            let mut question = draft();
            question.outcome = format!("Question {n}: {}", "ä".repeat(240));
            definitions.push(question);
        }
        state.work = Some(ResearchWorkState::new(
            "audit destination".to_owned(),
            definitions,
        )?);
        let original = state.work.clone();
        let context = state.work_context();
        let full = state.render_work_context(false);
        assert!(full.len() + "audit destination".len() + 1536 > 4096);
        println!(
            "research contract fixture: full={} bytes, partition={} bytes, reserve=1536 bytes",
            full.len(),
            context.len()
        );
        assert!(context.contains("Partitioned view"));
        assert!(context.len() + "audit destination".len() + 1536 <= 4096);
        for n in 1..=32 {
            assert!(context.contains(&format!("Q{n} Required Open")));
        }
        assert!(context.contains(&format!("ACTIVE Q1: Question 1: {}", "ä".repeat(240))));
        assert_eq!(context, state.work_context());
        assert_eq!(state.work, original);
        assert!(!state.work.as_ref().ok_or("work")?.ready_to_finish());
        Ok(())
    }

    #[test]
    fn research_aliases_require_identical_revisions_and_delivered_original_quotes() -> TestResult {
        let state = ResearchWorkState::new("audit destination".to_owned(), vec![draft()])?;
        let mut guard = guard(Some(state))?;
        let mut proposal = answer()?;
        proposal.evidence[0].source_ordinal = 2;
        let update = ResearchWorkUpdate {
            questions: vec![],
            results: vec![proposal],
        };
        assert!(guard.admit(&update).is_err());
        guard.aliases.push((2, guard.windows[0].revision.clone()));
        let admitted = guard.admit(&update).map_err(|_| "current alias rejected")?;
        assert_eq!(
            admitted.questions()[0].result().ok_or("result")?.sources()[0].source_id,
            guard.windows[0].source_id
        );
        guard.aliases[0].1 = FileRevision::new(
            guard.windows[0].revision.path().clone(),
            ContentHash::from_bytes([9; 32]),
        );
        assert!(guard.admit(&update).is_err());
        guard.aliases[0].1 = guard.windows[0].revision.clone();
        guard.windows.clear();
        assert!(guard.admit(&update).is_err());
        Ok(())
    }
}
