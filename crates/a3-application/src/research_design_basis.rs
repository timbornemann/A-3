//! Closed comparison policy for information handed to dependent design phases.

/// Native evaluation selection. Product callers retain prerequisite interpretations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResearchDesignBasis {
    /// Preserve budgeted source-bound interpretations and complete design decisions.
    #[default]
    Interpretations,
    /// Replace interpretation prose only when all its original ranges are delivered.
    Originals,
}
