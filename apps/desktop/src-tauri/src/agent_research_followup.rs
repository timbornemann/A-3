//! Deterministic read hints from a VALIDATED incomplete decision, never from raw model output.

use super::*;
use a3_application::AskResearchDecisionNote;

/// These are search candidates, not claims or a new capability. Every execution still uses the
/// existing action budget, pinned index and Safe Source Reader. Keep planning work bounded too.
pub(super) fn candidates(
    published: &a3_domain::PublishedIndex,
    state: &AskResearchWorkingSet,
    note: &AskResearchDecisionNote,
) -> Vec<AskResearchAction> {
    let hint = format!("{}\n{}", note.gap, note.next_step);
    let mut actions = Vec::new();
    for revision in resolved_target_revisions(&resolve_query_targets(published, &hint)) {
        if state.complete_files.contains(&revision) {
            continue;
        }
        if let Some(next) = state.next_file_pages.get(revision.path()) {
            actions.push(AskResearchAction::InspectPath {
                path: model_safe_path(revision.path()),
                start_line: *next,
            });
        } else {
            actions.push(AskResearchAction::InspectPath {
                path: model_safe_path(revision.path()),
                start_line: 1,
            });
        }
    }
    // Only names actually present in this publication may become a literal search. Ordinary
    // prose and repository instructions cannot turn into commands or invented paths.
    let tokens = hint
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| token.len() >= 3 && token.len() <= 128)
        .take(256)
        .collect::<BTreeSet<_>>();
    let mut names = BTreeSet::new();
    for symbol in published
        .publication()
        .graph()
        .symbols()
        .iter()
        .take(32_768)
    {
        let name = symbol.parsed().name().as_str();
        if tokens.contains(name)
            && matches!(
                symbol.parsed().kind(),
                a3_domain::SymbolKind::Function
                    | a3_domain::SymbolKind::Method
                    | a3_domain::SymbolKind::Class
                    | a3_domain::SymbolKind::Struct
                    | a3_domain::SymbolKind::Trait
                    | a3_domain::SymbolKind::Interface
                    | a3_domain::SymbolKind::Enum
                    | a3_domain::SymbolKind::TypeAlias
            )
        {
            names.insert(name.to_owned());
            if names.len() == 8 {
                break;
            }
        }
    }
    if !names.is_empty() {
        actions.push(AskResearchAction::SearchSourceText(
            names.into_iter().collect(),
        ));
    }
    // A cited, still paginated source is a concrete frontier even when the public gap does not
    // repeat its filename. Never invent a line number or walk unrelated files automatically.
    for ordinal in &note.source_ordinals {
        if let Some(source) = state.sources.get(usize::from(ordinal.saturating_sub(1)))
            && !state.complete_files.contains(source.revision())
            && let Some(next) = state.next_file_pages.get(source.revision().path())
        {
            let action = AskResearchAction::InspectPath {
                path: model_safe_path(source.revision().path()),
                start_line: *next,
            };
            if !actions.contains(&action) {
                actions.push(action);
            }
        }
    }
    actions
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResearchStopReason {
    TimeLimit,
    DecisionLimit,
    ActionLimit,
    Stagnation,
    EvidenceIncomplete,
    InvalidDecision,
    CitationRepair,
    ModelRetryLimit,
}

impl ResearchStopReason {
    pub(super) fn for_controller(error: a3_application::ResearchControllerError) -> Self {
        match error {
            a3_application::ResearchControllerError::TimedOut => Self::TimeLimit,
            a3_application::ResearchControllerError::ActionBudgetExhausted => Self::ActionLimit,
            _ => Self::DecisionLimit,
        }
    }

    pub(super) const fn message(self) -> &'static str {
        match self {
            Self::TimeLimit => "Die Zeitgrenze dieses Rechercheabschnitts ist erreicht.",
            Self::DecisionLimit => {
                "Die zulässigen Modellschritte dieses Rechercheabschnitts sind aufgebraucht."
            }
            Self::ActionLimit => {
                "Die zulässigen Leseaktionen dieses Rechercheabschnitts sind aufgebraucht."
            }
            Self::Stagnation => {
                "Zwei aufeinanderfolgende Leserunden haben keine neuen Belege geliefert."
            }
            Self::EvidenceIncomplete => {
                "Die Frage ist noch nicht vollständig belegt; im aktuellen Abschnitt sind keine weiteren Leserunden verfügbar."
            }
            Self::InvalidDecision => {
                "Der Modellschritt blieb nach seinem Einzelrepair ungültig oder das Reparaturbudget ist erschöpft; eine weitere sichere Recovery ist nicht verfügbar. Ungültige Leseaktionen wurden nicht ausgeführt."
            }
            Self::ModelRetryLimit => {
                "Die begrenzten Wiederholungen vorübergehend fehlgeschlagener Modellaufrufe sind ausgeschöpft."
            }
            Self::CitationRepair => {
                "Die Antwort konnte innerhalb des zulässigen Reparaturversuchs nicht vollständig ihren vorhandenen Quellen zugeordnet werden."
            }
        }
    }
}
