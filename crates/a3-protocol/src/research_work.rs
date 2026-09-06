//! Read-only V1 checklist projection. No actions, source text, or writable status fields.
use serde::{Deserialize, Serialize};

/// Requiredness in the immutable research contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResearchQuestionPriorityV1 {
    /// User-requested answer.
    Required,
    /// A prerequisite.
    Supporting,
    /// Non-blocking additional detail.
    Optional,
}
/// Core-owned state, separate from timeline animation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResearchQuestionStatusV1 {
    /// Not yet investigated.
    Open,
    /// Active question.
    Active,
    /// Admitted interpretation or design.
    Answered,
    /// Bounded unknown with an admitted explanation.
    Limited,
    /// Exhausted, still unanswered.
    Blocked,
    /// Source or prerequisite changed.
    Stale,
}
/// Result classification, never a machine-verified fact.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResearchResultKindV1 {
    /// Supported interpretation of original sources.
    Interpretation,
    /// Proposed new design.
    DesignDecision,
    /// Explicitly bounded unknown.
    BoundedUnknown,
}
/// One stable, user-facing question.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResearchQuestionV1 {
    /// Stable one-based identity.
    pub id: u16,
    /// Intended answer, not internal reasoning.
    pub outcome: String,
    /// Immutable requiredness.
    pub priority: ResearchQuestionPriorityV1,
    /// Current Core state.
    pub status: ResearchQuestionStatusV1,
    /// Earlier prerequisite identities.
    pub dependencies: Vec<u16>,
    /// Current or explicitly historical public result.
    pub result: Option<String>,
    /// Kind of the retained result, when present.
    pub result_kind: Option<ResearchResultKindV1>,
    /// Existing safe preview capabilities; no raw path or source body.
    pub source_refs: Vec<String>,
}
/// Versioned optional addition to historical work-trace details.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResearchWorkV1 {
    /// Exact nested schema version, currently one.
    pub schema_version: u8,
    /// Persisted monotone checkpoint revision.
    pub revision: u32,
    /// Stable question order, at most 32.
    pub questions: Vec<ResearchQuestionV1>,
}
