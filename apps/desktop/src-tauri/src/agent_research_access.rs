//! Canonical, content-free read receipts. No source alias or public note is an identity.
use super::*;
use a3_domain::{ContentHash, ResearchAccessKind, ResearchAccessOutcome};

pub(super) fn search_outcome(
    result: &a3_application::AskSourceTextSearchResult,
    indexed_files: usize,
) -> ResearchAccessOutcome {
    // Complete covers eligible files only. A skipped protected/binary file is not absence,
    // and a future permission change must not be suppressed by a negative receipt.
    if result.completeness() != AskResearchCompleteness::Complete
        || usize::from(result.files_examined()) != indexed_files
        || result.hits().len() > MAX_ADAPTIVE_SEARCH_SOURCES
    {
        ResearchAccessOutcome::Limited
    } else if result.hits().is_empty() {
        ResearchAccessOutcome::NoMatch
    } else {
        ResearchAccessOutcome::Completed
    }
}

pub(crate) fn scope(published: &a3_domain::PublishedIndex) -> ContentHash {
    let mut hash = blake3::Hasher::new();
    hash.update(b"a3.research-access-scope.v1\0");
    hash.update(published.run().id().as_bytes());
    hash.update(published.run().snapshot_id().as_bytes());
    ContentHash::from_bytes(*hash.finalize().as_bytes())
}

fn field(hash: &mut blake3::Hasher, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

impl AgentAskResearcher {
    pub(super) async fn append_access_checkpoint(
        &self,
        project: &ProjectIdentity,
        turn: &AskResearchTurn,
        state: &mut AskResearchWorkingSet,
        kind: ResearchAccessKind,
        outcome: Option<ResearchAccessOutcome>,
    ) -> Result<(), AgentSessionManagerFailure> {
        let work = state
            .work
            .as_ref()
            .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
        let kind = match kind {
            ResearchAccessKind::IndexInventory => "veröffentlichter Dateibestand",
            ResearchAccessKind::Inspect => "Originalseite",
            ResearchAccessKind::LiteralSearch => "Quelltextsuche",
            ResearchAccessKind::IndexSearch => "Indexauswahl",
            ResearchAccessKind::Directory => "Indexverzeichnis",
            ResearchAccessKind::Relations => "Beziehungen",
            ResearchAccessKind::Flow => "Funktionsablauf",
            ResearchAccessKind::Changes => "lokale Änderungen",
            ResearchAccessKind::Diagnostics => "Indexdiagnosen",
            ResearchAccessKind::Dependencies => "Abhängigkeiten",
            ResearchAccessKind::Tests => "Testbeziehungen",
            ResearchAccessKind::SecurityCandidates => "Sicherheitskandidaten",
        };
        let outcome = match outcome {
            None => "gestartet",
            Some(ResearchAccessOutcome::Completed) => {
                "ausgeführt, kein fachlicher Abschlussnachweis"
            }
            Some(ResearchAccessOutcome::NoMatch) => "keine Treffer im geprüften Bereich",
            Some(ResearchAccessOutcome::Unresolved) => {
                "Ziel im gebundenen Index nicht eindeutig auflösbar"
            }
            Some(ResearchAccessOutcome::Limited) => "durch eine feste Grenze beschränkt",
            Some(ResearchAccessOutcome::Unavailable) => "nicht verfügbar, kein negativer Beleg",
        };
        state.event_sequence = state.event_sequence.saturating_add(1);
        let event = research_event(
            turn.session_id(),
            turn.user_sequence(),
            state.event_sequence,
            AskResearchPhase::Reading,
            AskResearchState::Running,
            &format!("Lesezugriff {kind}: {outcome}"),
            None,
            AskResearchCompleteness::NotApplicable,
        )?
        .with_work_state(work.clone());
        self.trace.append_event(project, &event).await?;
        Ok(())
    }
}

fn revision(hash: &mut blake3::Hasher, revision: &a3_domain::FileRevision) {
    field(hash, revision.path().as_bytes());
    field(hash, revision.content_hash().as_bytes());
}

fn source(hash: &mut blake3::Hasher, state: &AskResearchWorkingSet, ordinal: u16) -> Option<()> {
    let source = state.sources.get(usize::from(ordinal.checked_sub(1)?))?;
    revision(hash, source.revision());
    field(hash, source.symbol().unwrap_or_default().as_bytes());
    field(hash, &[u8::from(source.range().is_some())]);
    if let Some(range) = source.range() {
        field(hash, &range.start_byte().to_le_bytes());
        field(hash, &range.end_byte().to_le_bytes());
    }
    Some(())
}

pub(super) fn identity(
    published: &a3_domain::PublishedIndex,
    state: &AskResearchWorkingSet,
    action: &AskResearchAction,
) -> Option<(ContentHash, ResearchAccessKind)> {
    let mut hash = blake3::Hasher::new();
    hash.update(b"a3.research-access.v1\0");
    let (tag, kind) = match action {
        AskResearchAction::InspectPath { path, start_line } => {
            if let Some(resolved) = resolve_index_path(published, path) {
                revision(&mut hash, resolved);
            } else {
                field(
                    &mut hash,
                    path.trim()
                        .replace('\\', "/")
                        .trim_start_matches("./")
                        .to_ascii_lowercase()
                        .as_bytes(),
                );
            }
            field(&mut hash, &start_line.to_le_bytes());
            (1, ResearchAccessKind::Inspect)
        }
        AskResearchAction::InspectSource(ordinal) => {
            let source = state.sources.get(usize::from(ordinal.checked_sub(1)?))?;
            revision(&mut hash, source.revision());
            let start = source.range().map_or(1, |r| {
                r.start_position()
                    .row()
                    .saturating_sub(12)
                    .saturating_add(1)
            });
            field(&mut hash, &start.to_le_bytes());
            // Both capabilities request the same 200-line safe page.
            (1, ResearchAccessKind::Inspect)
        }
        AskResearchAction::SearchSourceText(values) => {
            let mut values = values.iter().collect::<Vec<_>>();
            values.sort();
            values.dedup();
            for value in values {
                field(&mut hash, value.as_bytes());
            }
            (2, ResearchAccessKind::LiteralSearch)
        }
        AskResearchAction::SearchIndex(query) => {
            field(&mut hash, query.as_bytes());
            (3, ResearchAccessKind::IndexSearch)
        }
        AskResearchAction::ListDirectory(path) => {
            field(
                &mut hash,
                path.trim().trim_matches('/').replace('\\', "/").as_bytes(),
            );
            (4, ResearchAccessKind::Directory)
        }
        AskResearchAction::InspectRelations {
            source_ordinal,
            relation,
        } => {
            source(&mut hash, state, *source_ordinal)?;
            field(
                &mut hash,
                &[match relation {
                    AskResearchRelation::Callers => 1,
                    AskResearchRelation::Callees => 2,
                    AskResearchRelation::Imports => 3,
                    AskResearchRelation::Exports => 4,
                    AskResearchRelation::Tests => 5,
                }],
            );
            (5, ResearchAccessKind::Relations)
        }
        AskResearchAction::InspectFunctionFlow {
            source_ordinal,
            call_path,
            view,
        } => {
            source(&mut hash, state, *source_ordinal)?;
            field(&mut hash, &(call_path.len() as u64).to_le_bytes());
            for step in call_path {
                field(&mut hash, &step.get().to_le_bytes());
            }
            use a3_domain::FunctionFlowReadView;
            match view {
                FunctionFlowReadView::Steps(offset) => {
                    field(&mut hash, &[1]);
                    field(&mut hash, &offset.to_le_bytes());
                }
                FunctionFlowReadView::Values(offset) => {
                    field(&mut hash, &[2]);
                    field(&mut hash, &offset.to_le_bytes());
                }
                FunctionFlowReadView::Origins(value) => {
                    field(&mut hash, &[3]);
                    field(&mut hash, &value.get().to_le_bytes());
                }
                FunctionFlowReadView::Uses(value) => {
                    field(&mut hash, &[4]);
                    field(&mut hash, &value.get().to_le_bytes());
                }
            }
            (6, ResearchAccessKind::Flow)
        }
        AskResearchAction::InspectWorkingChanges => (7, ResearchAccessKind::Changes),
        AskResearchAction::QueryIndexDiagnostics => (8, ResearchAccessKind::Diagnostics),
        AskResearchAction::InspectDependencyGraph => (9, ResearchAccessKind::Dependencies),
        AskResearchAction::InspectTestTopology => (10, ResearchAccessKind::Tests),
        AskResearchAction::ScanSecurityCandidates => (11, ResearchAccessKind::SecurityCandidates),
    };
    field(&mut hash, &[tag]);
    Some((ContentHash::from_bytes(*hash.finalize().as_bytes()), kind))
}

fn empty_publication_inventory_key() -> ContentHash {
    let mut hash = blake3::Hasher::new();
    hash.update(b"a3.research-empty-publication-inventory.v1\0");
    ContentHash::from_bytes(*hash.finalize().as_bytes())
}

pub(super) fn is_empty_publication_inventory_result(
    work: &a3_domain::ResearchWorkState,
    question: a3_domain::ResearchQuestionId,
    result: &a3_domain::ResearchResult,
) -> bool {
    let Some(scope) = result.boundary() else {
        return false;
    };
    result.kind() == a3_domain::ResearchResultKind::BoundedUnknown
        && result.sources().is_empty()
        && work.accesses().iter().any(|access| {
            access.question == question
                && access.scope == scope
                && access.key == empty_publication_inventory_key()
                && access.kind == ResearchAccessKind::IndexInventory
                && access.outcome == Some(ResearchAccessOutcome::NoMatch)
        })
}

impl AskResearchWorkingSet {
    /// Returns whether the current complete Core plan is grounded by the exact empty
    /// publication inventory introduced by ADR-0111. A missing Task-Lens hit in a nonempty
    /// graph, a foreign scope, or an ordinary bounded unknown cannot satisfy this predicate.
    pub(super) fn has_empty_publication_inventory(
        &self,
        published: &a3_domain::PublishedIndex,
    ) -> bool {
        use a3_domain::ResearchQuestionId;
        if !published.publication().graph().files().is_empty()
            || !self.work_required_revisions.is_empty()
        {
            return false;
        }
        let Some(work) = &self.work else {
            return false;
        };
        if !work.ready_to_finish() || !super::research_work::core_plan_contract(work) {
            return false;
        }
        let scope = scope(published);
        let Some(inventory) = work
            .question(ResearchQuestionId::FIRST)
            .and_then(|question| question.result())
        else {
            return false;
        };
        inventory.boundary() == Some(scope)
            && is_empty_publication_inventory_result(work, ResearchQuestionId::FIRST, inventory)
    }

    /// ADR-0111: a published graph with zero files is a Core inventory, not an open gap.
    /// A nonempty graph that merely selected zero Task-Lens hits must not use this path.
    pub(super) fn close_empty_publication_inventory(
        &mut self,
        published: &a3_domain::PublishedIndex,
    ) -> Result<bool, AgentSessionManagerFailure> {
        use a3_domain::{ResearchResult, ResearchResultKind};
        if !published.publication().graph().files().is_empty()
            || !self.work_required_revisions.is_empty()
        {
            return Ok(false);
        }
        let Some(work) = &self.work else {
            return Ok(false);
        };
        let Some(id) = work.next_question() else {
            return Ok(false);
        };
        let question = work
            .question(id)
            .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
        if question.definition().kind != a3_domain::ResearchQuestionKind::Repository {
            return Ok(false);
        }
        let scope = scope(published);
        let mut next = work.clone();
        let inventory_key = empty_publication_inventory_key();
        if next
            .begin_access(id, scope, inventory_key, ResearchAccessKind::IndexInventory)
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?
        {
            next.finish_access(id, scope, inventory_key, ResearchAccessOutcome::NoMatch)
                .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        }
        let result = ResearchResult::new(
            ResearchResultKind::BoundedUnknown,
            "Der gebundene Indexstand enthält keine Dateien. Es gibt keine bestehenden Einstiegspunkte, APIs oder Integrationsgrenzen im Worktree. Das ist eine Core-Bestandsaufnahme des leeren Index, keine Laufzeitverifikation; später hinzugefügte oder nicht indizierte Dateien bleiben außerhalb dieses Ergebnisses.".to_owned(),
            Vec::new(),
            Some(scope),
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        next.exclude(id, scope)
            .and_then(|_| next.resolve(id, result))
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        self.work = Some(next);
        Ok(true)
    }

    /// Called only after the finite Core frontier is exhausted, never on budget/cancellation.
    /// Records a limitation of the performed investigation, not a claim of global absence.
    pub(super) fn close_investigated_boundary(
        &mut self,
        scope: ContentHash,
    ) -> Result<bool, AgentSessionManagerFailure> {
        use a3_domain::{ResearchResult, ResearchResultKind, ResearchResultSource};
        let Some(work) = &self.work else {
            return Ok(false);
        };
        let Some(id) = work.next_question() else {
            return Ok(false);
        };
        let question = work
            .question(id)
            .ok_or(AgentSessionManagerFailure::InvalidOutput)?;
        let receipts = work
            .accesses()
            .iter()
            .filter(|a| a.question == id && a.scope == scope)
            .collect::<Vec<_>>();
        if question.definition().kind != a3_domain::ResearchQuestionKind::Repository
            || question.attempts().is_empty()
            || receipts.iter().any(|a| {
                !matches!(
                    a.outcome,
                    Some(
                        ResearchAccessOutcome::Completed
                            | ResearchAccessOutcome::NoMatch
                            | ResearchAccessOutcome::Unresolved
                    )
                )
            })
            || !receipts.iter().any(|a| {
                a.kind == ResearchAccessKind::LiteralSearch
                    && a.outcome == Some(ResearchAccessOutcome::NoMatch)
            })
            || !receipts.iter().any(|a| {
                matches!(
                    (a.kind, a.outcome),
                    (
                        ResearchAccessKind::Inspect,
                        Some(ResearchAccessOutcome::Unresolved)
                    ) | (
                        ResearchAccessKind::Directory,
                        Some(ResearchAccessOutcome::NoMatch)
                    )
                )
            })
        {
            return Ok(false);
        }
        let mut sources = Vec::new();
        for window in self.work_evidence_windows() {
            let source = ResearchResultSource {
                source_id: window.source_id,
                revision: window.revision.clone(),
                range: window.range,
            };
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
        if self
            .work_required_revisions
            .iter()
            .any(|revision| !sources.iter().any(|s| &s.revision == revision))
        {
            return Ok(false);
        }
        // The question's boundary is the publication scope, not an inferred scope from its
        // entire access history. The same atomic checkpoint retains the exact supporting
        // receipts. Older scopes remain auditable but cannot authorize or poison this result.
        let boundary = scope;
        let text = format!(
            "Begrenzte Erkenntnis: Die Teilfrage konnte im geprüften Indexstand nicht geklärt werden. {} abgeschlossene Lesezugriffe umfassen eine vollständige Literalsuche ohne Treffer und einen nicht auflösbaren Pfad oder ein leeres Indexverzeichnis. Der verfügbare begrenzte Rechercheweg ist ausgeschöpft. Das belegt weder die allgemeine Nichtexistenz noch das Laufzeitverhalten; externe, dynamische und nicht indizierte Ziele bleiben ungeklärt.",
            receipts.len()
        );
        let result = ResearchResult::new(
            ResearchResultKind::BoundedUnknown,
            text,
            sources,
            Some(boundary),
        )
        .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        let mut next = work.clone();
        next.exclude(id, boundary)
            .and_then(|_| next.resolve(id, result))
            .map_err(|_| AgentSessionManagerFailure::InvalidOutput)?;
        self.work = Some(next);
        Ok(true)
    }

    pub(super) fn novel_work_accesses(
        &self,
        published: &a3_domain::PublishedIndex,
        actions: Vec<AskResearchAction>,
    ) -> Vec<AskResearchAction> {
        let Some((work, question)) = self
            .work
            .as_ref()
            .and_then(|w| w.next_question().map(|q| (w, q)))
        else {
            return actions;
        };
        let scope = scope(published);
        let mut seen = BTreeSet::new();
        actions
            .into_iter()
            .filter(|a| {
                identity(published, self, a).is_none_or(|(key, _)| {
                    !work.access_excluded(question, scope, key) && seen.insert(key)
                })
            })
            .collect()
    }

    /// Preserve incomplete/failure information across actions composed of several safe reads.
    pub(super) fn observe_access(&mut self, outcome: ResearchAccessOutcome) {
        fn rank(outcome: ResearchAccessOutcome) -> u8 {
            match outcome {
                ResearchAccessOutcome::Completed => 0,
                ResearchAccessOutcome::NoMatch => 1,
                ResearchAccessOutcome::Unresolved => 2,
                ResearchAccessOutcome::Limited => 3,
                ResearchAccessOutcome::Unavailable => 4,
            }
        }
        if rank(outcome) > rank(self.access_outcome) {
            self.access_outcome = outcome;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{
        ResearchQuestionDraft, ResearchQuestionId, ResearchQuestionKind, ResearchQuestionPriority,
        ResearchQuestionStatus, ResearchWorkState,
    };

    #[test]
    fn research_boundary_needs_independent_completed_receipts_and_revalidates_scope()
    -> Result<(), Box<dyn Error>> {
        let mut state = AskResearchWorkingSet::new(4096);
        state.work = Some(ResearchWorkState::new(
            "Locate absent plugin".to_owned(),
            vec![ResearchQuestionDraft {
                request_fragment: "absent plugin".to_owned(),
                outcome: "Locate absent plugin".to_owned(),
                priority: ResearchQuestionPriority::Required,
                kind: ResearchQuestionKind::Repository,
                dependencies: vec![],
            }],
        )?);
        let id = ResearchQuestionId::FIRST;
        let scope = ContentHash::from_bytes([1; 32]);
        assert!(!state.close_investigated_boundary(scope)?);
        state
            .work
            .as_mut()
            .ok_or("work")?
            .begin_analysis(id, ContentHash::from_bytes([2; 32]))?;
        for (key, kind, outcome) in [
            (
                3,
                ResearchAccessKind::LiteralSearch,
                ResearchAccessOutcome::NoMatch,
            ),
            (
                4,
                ResearchAccessKind::Inspect,
                ResearchAccessOutcome::Unresolved,
            ),
        ] {
            assert!(!state.close_investigated_boundary(scope)?);
            let work = state.work.as_mut().ok_or("work")?;
            work.begin_access(id, scope, ContentHash::from_bytes([key; 32]), kind)?;
            assert!(!state.close_investigated_boundary(scope)?);
            state.work.as_mut().ok_or("work")?.finish_access(
                id,
                scope,
                ContentHash::from_bytes([key; 32]),
                outcome,
            )?;
        }
        let before = state.work.clone();
        for outcome in [
            None,
            Some(ResearchAccessOutcome::Limited),
            Some(ResearchAccessOutcome::Unavailable),
        ] {
            state.work = before.clone();
            let work = state.work.as_mut().ok_or("work")?;
            work.begin_access(
                id,
                scope,
                ContentHash::from_bytes([5; 32]),
                ResearchAccessKind::Inspect,
            )?;
            if let Some(outcome) = outcome {
                work.finish_access(id, scope, ContentHash::from_bytes([5; 32]), outcome)?;
            }
            assert!(!state.close_investigated_boundary(scope)?);
        }
        state.work = before;
        assert!(!state.close_investigated_boundary(ContentHash::from_bytes([8; 32]))?);
        assert!(state.close_investigated_boundary(scope)?);
        let work = state.work.as_mut().ok_or("work")?;
        assert!(work.ready_to_finish());
        assert_eq!(
            work.question(id).ok_or("question")?.status(),
            ResearchQuestionStatus::Limited
        );
        assert!(!work.revalidate_in_scope(&[], Some(scope))?);
        assert!(work.revalidate_in_scope(&[], Some(ContentHash::from_bytes([9; 32])))?);
        assert!(!work.ready_to_finish());
        assert_eq!(
            work.question(id).ok_or("question")?.status(),
            ResearchQuestionStatus::Stale
        );
        let next_scope = ContentHash::from_bytes([9; 32]);
        work.begin_analysis(id, ContentHash::from_bytes([10; 32]))?;
        for (key, kind, outcome) in [
            (
                3,
                ResearchAccessKind::LiteralSearch,
                ResearchAccessOutcome::NoMatch,
            ),
            (
                4,
                ResearchAccessKind::Inspect,
                ResearchAccessOutcome::Unresolved,
            ),
        ] {
            work.begin_access(id, next_scope, ContentHash::from_bytes([key; 32]), kind)?;
            work.finish_access(id, next_scope, ContentHash::from_bytes([key; 32]), outcome)?;
        }
        assert!(
            state.close_investigated_boundary(next_scope)?,
            "historical receipts must not poison a freshly investigated scope"
        );
        let work = state.work.as_mut().ok_or("work")?;
        assert_eq!(
            work.accesses().len(),
            4,
            "old investigation remains auditable"
        );
        assert!(!work.revalidate_in_scope(&[], Some(next_scope))?);
        assert!(
            work.revalidate_in_scope(&[], Some(scope))?,
            "historical receipts cannot make a new result valid in an older scope"
        );
        Ok(())
    }
    #[test]
    fn research_search_receipts_never_turn_skipped_files_into_negative_evidence()
    -> Result<(), Box<dyn Error>> {
        let result = a3_application::AskSourceTextSearchResult::new(
            vec![],
            1,
            10,
            AskResearchCompleteness::Complete,
        )?;
        assert_eq!(search_outcome(&result, 1), ResearchAccessOutcome::NoMatch);
        assert_eq!(search_outcome(&result, 2), ResearchAccessOutcome::Limited);
        assert_eq!(search_outcome(&result, 0), ResearchAccessOutcome::Limited);
        let empty_index = a3_application::AskSourceTextSearchResult::new(
            vec![],
            0,
            0,
            AskResearchCompleteness::Complete,
        )?;
        assert_eq!(
            search_outcome(&empty_index, 0),
            ResearchAccessOutcome::NoMatch
        );
        let incomplete = a3_application::AskSourceTextSearchResult::new(
            vec![],
            1,
            10,
            AskResearchCompleteness::Limited,
        )?;
        assert_eq!(
            search_outcome(&incomplete, 1),
            ResearchAccessOutcome::Limited
        );
        let mut state = AskResearchWorkingSet::new(4096);
        state.observe_access(ResearchAccessOutcome::Unavailable);
        state.observe_access(ResearchAccessOutcome::NoMatch);
        assert_eq!(state.access_outcome, ResearchAccessOutcome::Unavailable);
        Ok(())
    }

    #[test]
    fn empty_published_index_closes_repository_inventory_and_keeps_files_as_a_blocker()
    -> Result<(), Box<dyn Error>> {
        let mut state = AskResearchWorkingSet::new(4096);
        state.initialize_plan_work("erstelle einen kleinen python server")?;
        let empty = published_index(Vec::new())?;
        assert!(state.close_empty_publication_inventory(&empty)?);
        state.design_basis = a3_application::ResearchDesignBasis::Originals;
        assert!(state.uses_original_design_basis());
        assert!(
            state.design_originals_delivered(),
            "the typed inventory receipt closes the otherwise empty original basis"
        );
        assert!(
            !state.has_empty_publication_inventory(&empty),
            "inventory alone cannot ground an incomplete design"
        );
        {
            let work = state.work.as_ref().ok_or("work")?;
            let first = work.question(ResearchQuestionId::FIRST).ok_or("question")?;
            assert_eq!(first.status(), ResearchQuestionStatus::Limited);
            assert_eq!(
                first.result().ok_or("result")?.kind(),
                a3_domain::ResearchResultKind::BoundedUnknown
            );
            assert!(first.result().ok_or("result")?.sources().is_empty());
            assert_eq!(work.next_question(), Some(ResearchQuestionId::new(2)?));
        }
        assert!(!state.close_empty_publication_inventory(&empty)?);
        assert!(
            !state
                .work
                .as_mut()
                .ok_or("work")?
                .revalidate_in_scope(&[], Some(scope(&empty)))?
        );

        let mut blocked = AskResearchWorkingSet::new(4096);
        blocked.initialize_plan_work("erstelle einen kleinen python server")?;
        let file = a3_domain::FileRevision::new(
            a3_domain::RepositoryPath::try_from_bytes(b"hello.py".to_vec())?,
            ContentHash::from_bytes([9; 32]),
        );
        assert!(!blocked.close_empty_publication_inventory(&published_index(vec![file.clone()])?)?);
        assert!(!blocked.has_empty_publication_inventory(&published_index(vec![file.clone()])?));
        assert_eq!(
            blocked
                .work
                .as_ref()
                .ok_or("work")?
                .question(ResearchQuestionId::FIRST)
                .ok_or("question")?
                .status(),
            ResearchQuestionStatus::Open
        );

        let mut selected_none = AskResearchWorkingSet::new(4096);
        selected_none.initialize_plan_work("erstelle einen kleinen python server")?;
        selected_none.work_required_revisions.push(file);
        assert!(
            !selected_none.close_empty_publication_inventory(&empty)?,
            "a nonempty required set is not an empty publication"
        );

        Ok(())
    }

    fn published_index(
        files: Vec<a3_domain::FileRevision>,
    ) -> Result<a3_domain::PublishedIndex, Box<dyn Error>> {
        let snapshot_id = a3_domain::SnapshotId::from_bytes([11; 32]);
        let graph = a3_domain::LinkedGraph::new(
            snapshot_id,
            files.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )?;
        let ranking = a3_domain::RankProjection::new(
            snapshot_id,
            a3_domain::RankingPolicyVersion::v1(),
            Vec::new(),
        )?;
        let policy = a3_domain::ModulePolicyVersion::v1();
        let card = a3_domain::RepositoryCard::new(
            snapshot_id,
            policy,
            Vec::new(),
            Vec::new(),
            a3_domain::ModuleSymbolSet::empty(),
            u32::try_from(files.len())?,
            0,
        )?;
        let modules =
            a3_domain::ModuleProjection::new(snapshot_id, policy, Vec::new(), Vec::new(), card)?;
        let publication = a3_domain::IndexPublication::new(graph, ranking, files, modules)?;
        let run = a3_domain::IndexRunRecord::new(
            a3_domain::IndexRunId::from_bytes([12; 32]),
            snapshot_id,
            a3_domain::RankingPolicyVersion::v1(),
            a3_domain::IndexRunSequence::new(1)?,
            a3_domain::IndexRunStatus::Published,
        );
        Ok(a3_domain::PublishedIndex::new(run, publication)?)
    }
}
