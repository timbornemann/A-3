use super::SymbolId;
use crate::{FileRevision, ParsedSymbol};

/// One parsed symbol promoted into snapshot-linkable global identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSymbol {
    id: SymbolId,
    revision: FileRevision,
    parsed: ParsedSymbol,
}

impl GraphSymbol {
    /// Binds a deterministic global ID to the exact parsed file revision.
    #[must_use]
    pub const fn new(id: SymbolId, revision: FileRevision, parsed: ParsedSymbol) -> Self {
        Self {
            id,
            revision,
            parsed,
        }
    }

    /// Returns the revision-stable global symbol ID.
    #[must_use]
    pub const fn id(&self) -> SymbolId {
        self.id
    }

    /// Returns the exact path and content hash that supplied the declaration.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }

    /// Returns the adapter-normalized symbol projection.
    #[must_use]
    pub const fn parsed(&self) -> &ParsedSymbol {
        &self.parsed
    }
}
