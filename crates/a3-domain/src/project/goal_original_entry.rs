const MAX_ENTRY_ORDINAL: usize = 64;

/// Kind of a non-criterion entry within one immutable Goal Contract revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GoalOriginalEntryKind {
    /// A mandatory execution or product boundary.
    Constraint,
    /// An explicitly excluded outcome.
    NonGoal,
    /// A choice already confirmed by the user.
    UserDecision,
}

/// One-based ordinal, at most 64, within one kind of goal entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalOriginalEntryOrdinal(u8);

impl GoalOriginalEntryOrdinal {
    /// Returns the validated one-based position within the original goal collection.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Content-free collection and position, not a new independent durable identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalOriginalEntry {
    kind: GoalOriginalEntryKind,
    ordinal: GoalOriginalEntryOrdinal,
}

impl GoalOriginalEntry {
    pub(super) fn from_index(kind: GoalOriginalEntryKind, index: usize) -> Option<Self> {
        let ordinal = index.checked_add(1)?;
        if ordinal > MAX_ENTRY_ORDINAL {
            return None;
        }
        Some(Self {
            kind,
            ordinal: GoalOriginalEntryOrdinal(u8::try_from(ordinal).ok()?),
        })
    }

    /// Returns the original goal collection.
    #[must_use]
    pub const fn kind(self) -> GoalOriginalEntryKind {
        self.kind
    }

    /// Returns the one-based position within that collection.
    #[must_use]
    pub const fn ordinal(self) -> GoalOriginalEntryOrdinal {
        self.ordinal
    }
}
