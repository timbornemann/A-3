use super::{
    GoalContract, GoalContractReference, GoalOriginalBindingError, GoalOriginalDigest,
    GoalOriginalEntry, GoalOriginalEntryKind, GoalOriginalError, GoalOriginalRange,
};
use std::error::Error;
use std::fmt;

/// Derived structural original coverage, never a semantic truth or success receipt.
///
/// The report is rebuilt from the authoritative goal. It introduces no persisted state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalOriginalCoverage {
    goal: GoalContractReference,
    digest: GoalOriginalDigest,
    covered_bytes: u32,
    gaps: Vec<GoalOriginalRange>,
    unbound_entries: Vec<GoalOriginalEntry>,
}

impl GoalOriginalCoverage {
    /// Unions all present entry ranges and reports every remaining byte as an ordered gap.
    ///
    /// Whitespace, punctuation and other glyphs receive no implicit formatting exemption.
    /// Constraints, non-goals and user decisions without an origin remain unbound entries,
    /// even when another entry's range already covers their text.
    pub fn derive_from(goal: &GoalContract) -> Result<Self, GoalOriginalCoverageError> {
        goal.validate_original_binding()
            .map_err(GoalOriginalCoverageError::InvalidBinding)?;
        let original = goal
            .original()
            .ok_or(GoalOriginalCoverageError::MissingOriginal)?;
        let mut ranges = Vec::new();
        for criterion in goal.draft().acceptance_criteria() {
            let origin = criterion
                .origin()
                .ok_or(GoalOriginalCoverageError::InvalidBinding(
                    GoalOriginalBindingError::MissingCriterionOrigin {
                        criterion_id: criterion.id(),
                    },
                ))?;
            ranges.push(origin.original_range());
        }
        let mut unbound_entries = Vec::new();
        collect_entries(
            GoalOriginalEntryKind::Constraint,
            goal.draft()
                .constraints()
                .iter()
                .map(|entry| entry.origin()),
            &mut ranges,
            &mut unbound_entries,
        )?;
        collect_entries(
            GoalOriginalEntryKind::NonGoal,
            goal.draft().non_goals().iter().map(|entry| entry.origin()),
            &mut ranges,
            &mut unbound_entries,
        )?;
        collect_entries(
            GoalOriginalEntryKind::UserDecision,
            goal.draft()
                .user_decisions()
                .iter()
                .map(|entry| entry.origin()),
            &mut ranges,
            &mut unbound_entries,
        )?;
        ranges.sort_unstable_by_key(|range| (range.start_byte(), range.end_byte()));
        let mut cursor = 0_u32;
        let mut covered_bytes = 0_u32;
        let mut gaps = Vec::new();
        for range in ranges {
            let start = range.start_byte();
            let end = range.end_byte();
            if start > cursor {
                gaps.push(
                    original
                        .range(cursor as usize, start as usize)
                        .map_err(GoalOriginalCoverageError::InvalidRange)?,
                );
            }
            if end > cursor {
                covered_bytes = covered_bytes
                    .checked_add(end - start.max(cursor))
                    .ok_or(GoalOriginalCoverageError::ByteCountOverflow)?;
                cursor = end;
            }
        }
        if (cursor as usize) < original.as_bytes().len() {
            gaps.push(
                original
                    .range(cursor as usize, original.as_bytes().len())
                    .map_err(GoalOriginalCoverageError::InvalidRange)?,
            );
        }
        Ok(Self {
            goal: goal.reference(),
            digest: original.digest(),
            covered_bytes,
            gaps,
            unbound_entries,
        })
    }

    /// Returns the exact task and immutable revision from which this report was derived.
    #[must_use]
    pub const fn goal(&self) -> GoalContractReference {
        self.goal
    }

    /// Returns the digest of the unchanged authoritative original.
    #[must_use]
    pub const fn digest(&self) -> GoalOriginalDigest {
        self.digest
    }

    /// Returns unique covered UTF-8 bytes; overlapping criterion ranges count once.
    #[must_use]
    pub const fn covered_bytes(&self) -> u32 {
        self.covered_bytes
    }

    /// Returns all uncovered bytes in original order as nonoverlapping UTF-8-safe ranges.
    #[must_use]
    pub fn gaps(&self) -> &[GoalOriginalRange] {
        &self.gaps
    }

    /// Returns unannotated collections in Constraint/NonGoal/UserDecision and ordinal order.
    #[must_use]
    pub fn unbound_entries(&self) -> &[GoalOriginalEntry] {
        &self.unbound_entries
    }

    /// Returns only structural completeness, not semantic adequacy or verified implementation.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.gaps.is_empty() && self.unbound_entries.is_empty()
    }
}

/// Content-free reason why structural coverage could not be derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalOriginalCoverageError {
    /// A legacy goal has no authoritative original to cover.
    MissingOriginal,
    /// The goal and one of its source bindings do not match.
    InvalidBinding(GoalOriginalBindingError),
    /// A computed gap could not be represented against the exact original.
    InvalidRange(GoalOriginalError),
    /// Coverage arithmetic exceeded its bounded byte representation.
    ByteCountOverflow,
    /// A collection exceeded the supported 64-entry ordinal boundary.
    UnboundEntryLimitExceeded,
}

impl fmt::Display for GoalOriginalCoverageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingOriginal => "original coverage requires an authoritative original",
            Self::InvalidBinding(_) => "original coverage has an invalid source binding",
            Self::InvalidRange(_) => "original coverage has an invalid gap range",
            Self::ByteCountOverflow => "original coverage byte count overflowed",
            Self::UnboundEntryLimitExceeded => "original coverage has too many unbound entries",
        })
    }
}

impl Error for GoalOriginalCoverageError {}

fn collect_entries(
    kind: GoalOriginalEntryKind,
    origins: impl Iterator<Item = Option<GoalOriginalRange>>,
    ranges: &mut Vec<GoalOriginalRange>,
    unbound: &mut Vec<GoalOriginalEntry>,
) -> Result<(), GoalOriginalCoverageError> {
    for (index, origin) in origins.enumerate() {
        let entry = GoalOriginalEntry::from_index(kind, index)
            .ok_or(GoalOriginalCoverageError::UnboundEntryLimitExceeded)?;
        if let Some(range) = origin {
            ranges.push(range);
        } else {
            unbound.push(entry);
        }
    }
    Ok(())
}
