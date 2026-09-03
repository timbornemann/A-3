use std::fmt;

/// User-selected bounded research depth for one submitted message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentResearchDepth {
    /// Fast default with up to six decisions and twelve reads.
    Standard,
    /// Larger explicit budget with up to twelve decisions and twenty-four reads.
    Thorough,
}

/// Closed user-facing phase of one bounded Ask research turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AskResearchPhase {
    /// The current published index and explicit references are being resolved.
    Preparing,
    /// Candidate names, paths, symbols, and relations are being localized.
    Locating,
    /// The next bounded read-only action batch is being selected.
    Deciding,
    /// Current hash-bound source or graph information is being read.
    Reading,
    /// Newly read Evidence is being evaluated against the open gap.
    Evaluating,
    /// A final answer, question, or plan is being produced.
    AnsweringOrPlanning,
    /// Deterministic Task Lens channels are selecting likely evidence.
    SelectingEvidence,
    /// Current repository text is being searched with fixed read-only limits.
    SearchingSource,
    /// A current file, symbol, relationship, or claim is being inspected.
    InspectingSource,
    /// The model is producing or validating the evidence-grounded answer.
    Answering,
    /// The answer and cited source set were committed atomically.
    Completed,
}

/// Durable terminal or active state of one Ask research turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AskResearchState {
    /// Research is still owned by the active conversation job.
    Running,
    /// The answer and citations were committed.
    Completed,
    /// The bounded research or answer could not complete.
    Failed,
    /// The user cancelled the owning conversation job.
    Cancelled,
    /// The bounded section retained evidence and needs explicit user continuation.
    AwaitingContinuation,
}

/// Closed explanation for why a source entered the temporary Ask context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AskResearchSelectionReason {
    /// The user explicitly named an exact repository path or symbol.
    ExactNameOrPath,
    /// Deterministic indexed text matched the question.
    IndexedText,
    /// A current graph relationship connected the source to another result.
    Relationship,
    /// The source was selected as associated test evidence.
    Test,
    /// A current verified Module Card claim supplied the source.
    VerifiedModuleKnowledge,
    /// Similarity proposed the candidate and a later source read verified it.
    SemanticCandidate,
    /// A bounded literal scan found the text in current source.
    SourceText,
}

/// Closed kind of evidence that may be disclosed by Ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AskResearchSourceKind {
    /// Current file evidence.
    File,
    /// Current declaration or source-span evidence.
    Symbol,
    /// Current graph relationship evidence.
    Relationship,
    /// Current verified Module Card claim evidence.
    VerifiedClaim,
}

/// Whether a bounded search covered its declared repository scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AskResearchCompleteness {
    /// The search reached the end of its declared scope.
    Complete,
    /// One or more fixed safety or resource limits stopped the search.
    Limited,
    /// The event did not perform a repository-wide search.
    NotApplicable,
}

impl fmt::Display for AskResearchPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Preparing => "preparing",
            Self::Locating => "locating",
            Self::Deciding => "deciding",
            Self::Reading => "reading",
            Self::Evaluating => "evaluating",
            Self::AnsweringOrPlanning => "answering-or-planning",
            Self::SelectingEvidence => "selecting-evidence",
            Self::SearchingSource => "searching-source",
            Self::InspectingSource => "inspecting-source",
            Self::Answering => "answering",
            Self::Completed => "completed",
        })
    }
}
