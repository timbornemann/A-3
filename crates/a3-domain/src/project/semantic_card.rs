use super::SnapshotId;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_RAW_SEMANTIC_CARD_BYTES: usize = 65_536;
const MAX_NORMALIZED_SEMANTIC_CARD_BYTES: usize = 16_384;
const MAX_SEMANTIC_CARDS_PER_BATCH: usize = 512;

/// Stable logical identity of one semantic card across body revisions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SemanticCardId([u8; 32]);

impl SemanticCardId {
    /// Constructs an ID from a versioned card producer.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for SemanticCardId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(formatter, &self.0)
    }
}

impl fmt::Debug for SemanticCardId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SemanticCardId({self})")
    }
}

/// Digest of the exact normalized card body submitted for embedding.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyHash([u8; 32]);

impl BodyHash {
    /// Constructs a digest reconstructed from trusted persistence.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for BodyHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(formatter, &self.0)
    }
}

impl fmt::Debug for BodyHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "BodyHash({self})")
    }
}

/// Version governing whitespace canonicalization and body-hash derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SemanticCardNormalizationVersion(u16);

impl SemanticCardNormalizationVersion {
    /// Deterministic version-one normalization.
    pub const V1: Self = Self(1);

    /// Returns the persisted positive version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// One bounded canonical semantic projection suitable for hashing and embedding.
#[derive(Clone, PartialEq, Eq)]
pub struct NormalizedSemanticCard {
    id: SemanticCardId,
    snapshot_id: SnapshotId,
    normalization_version: SemanticCardNormalizationVersion,
    body: String,
    body_hash: BodyHash,
}

impl NormalizedSemanticCard {
    /// Canonicalizes a version-one card body and derives its domain-separated hash.
    pub fn normalize_v1(
        id: SemanticCardId,
        snapshot_id: SnapshotId,
        raw_body: &str,
    ) -> Result<Self, SemanticCardNormalizationError> {
        if raw_body.len() > MAX_RAW_SEMANTIC_CARD_BYTES {
            return Err(SemanticCardNormalizationError::RawBodyTooLarge {
                actual: raw_body.len(),
            });
        }
        let body = normalize_body(raw_body)?;
        if body.is_empty() {
            return Err(SemanticCardNormalizationError::EmptyBody);
        }
        if body.len() > MAX_NORMALIZED_SEMANTIC_CARD_BYTES {
            return Err(SemanticCardNormalizationError::NormalizedBodyTooLarge {
                actual: body.len(),
            });
        }
        let body_hash = hash_body(SemanticCardNormalizationVersion::V1, body.as_bytes());
        Ok(Self {
            id,
            snapshot_id,
            normalization_version: SemanticCardNormalizationVersion::V1,
            body,
            body_hash,
        })
    }

    /// Returns the stable logical card identity.
    #[must_use]
    pub const fn id(&self) -> SemanticCardId {
        self.id
    }

    /// Returns the immutable snapshot from which the card was produced.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the normalization and hashing policy version.
    #[must_use]
    pub const fn normalization_version(&self) -> SemanticCardNormalizationVersion {
        self.normalization_version
    }

    /// Returns the canonical body submitted to an embedding provider.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns the digest used as the semantic-cache revision key.
    #[must_use]
    pub const fn body_hash(&self) -> BodyHash {
        self.body_hash
    }
}

impl fmt::Debug for NormalizedSemanticCard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NormalizedSemanticCard")
            .field("id", &self.id)
            .field("snapshot_id", &self.snapshot_id)
            .field("normalization_version", &self.normalization_version)
            .field("body_bytes", &self.body.len())
            .field("body_hash", &self.body_hash)
            .finish()
    }
}

/// Canonically ordered cards for one bounded snapshot-local embedding job.
#[derive(Clone, PartialEq, Eq)]
pub struct SemanticCardBatch {
    snapshot_id: SnapshotId,
    cards: Vec<NormalizedSemanticCard>,
}

impl SemanticCardBatch {
    /// Validates snapshot ownership, cardinality, and stable logical uniqueness.
    pub fn new(
        snapshot_id: SnapshotId,
        mut cards: Vec<NormalizedSemanticCard>,
    ) -> Result<Self, SemanticCardBatchError> {
        if cards.len() > MAX_SEMANTIC_CARDS_PER_BATCH {
            return Err(SemanticCardBatchError::TooManyCards {
                actual: cards.len(),
            });
        }
        if cards.iter().any(|card| card.snapshot_id() != snapshot_id) {
            return Err(SemanticCardBatchError::SnapshotMismatch);
        }
        let mut identities = BTreeSet::new();
        if cards.iter().any(|card| !identities.insert(card.id())) {
            return Err(SemanticCardBatchError::DuplicateCardId);
        }
        cards.sort_by_key(NormalizedSemanticCard::id);
        Ok(Self { snapshot_id, cards })
    }

    /// Returns the sole snapshot represented by every card.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns cards in stable logical-ID order.
    #[must_use]
    pub fn cards(&self) -> &[NormalizedSemanticCard] {
        &self.cards
    }

    /// Returns whether this is a valid no-op batch.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Returns the bounded number of cards.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.cards.len()
    }
}

impl fmt::Debug for SemanticCardBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SemanticCardBatch")
            .field("snapshot_id", &self.snapshot_id)
            .field("card_count", &self.cards.len())
            .finish()
    }
}

/// Invalid untrusted semantic-card body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCardNormalizationError {
    /// Raw input exceeded the pre-normalization allocation boundary.
    RawBodyTooLarge {
        /// Observed UTF-8 byte count.
        actual: usize,
    },
    /// Canonicalization removed all meaningful content.
    EmptyBody,
    /// Canonical output exceeded the provider-input boundary.
    NormalizedBodyTooLarge {
        /// Canonical UTF-8 byte count.
        actual: usize,
    },
    /// Input contained a control character other than line endings or horizontal tab.
    UnsupportedControlCharacter,
}

impl fmt::Display for SemanticCardNormalizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RawBodyTooLarge { actual } => write!(
                formatter,
                "raw semantic card has {actual} bytes; maximum is {MAX_RAW_SEMANTIC_CARD_BYTES}"
            ),
            Self::EmptyBody => {
                formatter.write_str("semantic card body is empty after normalization")
            }
            Self::NormalizedBodyTooLarge { actual } => write!(
                formatter,
                "normalized semantic card has {actual} bytes; maximum is {MAX_NORMALIZED_SEMANTIC_CARD_BYTES}"
            ),
            Self::UnsupportedControlCharacter => {
                formatter.write_str("semantic card contains an unsupported control character")
            }
        }
    }
}

impl Error for SemanticCardNormalizationError {}

/// Invalid collection submitted to one embedding batch job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCardBatchError {
    /// More cards were supplied than one bounded job accepts.
    TooManyCards {
        /// Observed card count.
        actual: usize,
    },
    /// At least one card came from another snapshot.
    SnapshotMismatch,
    /// A logical card identity appeared more than once.
    DuplicateCardId,
}

impl fmt::Display for SemanticCardBatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyCards { actual } => write!(
                formatter,
                "semantic card batch has {actual} cards; maximum is {MAX_SEMANTIC_CARDS_PER_BATCH}"
            ),
            Self::SnapshotMismatch => {
                formatter.write_str("semantic card batch mixes immutable snapshots")
            }
            Self::DuplicateCardId => {
                formatter.write_str("semantic card batch contains a duplicate logical card")
            }
        }
    }
}

impl Error for SemanticCardBatchError {}

fn normalize_body(raw_body: &str) -> Result<String, SemanticCardNormalizationError> {
    if raw_body
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
    {
        return Err(SemanticCardNormalizationError::UnsupportedControlCharacter);
    }

    let canonical_newlines = raw_body.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut pending_blank = false;
    for line in canonical_newlines.lines() {
        let normalized_line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized_line.is_empty() {
            if !lines.is_empty() {
                pending_blank = true;
            }
            continue;
        }
        if pending_blank {
            lines.push(String::new());
            pending_blank = false;
        }
        lines.push(normalized_line);
    }
    Ok(lines.join("\n"))
}

fn hash_body(version: SemanticCardNormalizationVersion, body: &[u8]) -> BodyHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"a3-semantic-card-body\0");
    hasher.update(&version.get().to_be_bytes());
    hasher.update(body);
    BodyHash(*hasher.finalize().as_bytes())
}

fn write_hex(formatter: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        NormalizedSemanticCard, SemanticCardBatch, SemanticCardBatchError, SemanticCardId,
        SemanticCardNormalizationError,
    };
    use crate::SnapshotId;

    #[test]
    fn v1_normalization_is_bounded_canonical_and_body_hash_based()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = SnapshotId::from_bytes([1; 32]);
        let first = NormalizedSemanticCard::normalize_v1(
            SemanticCardId::from_bytes([2; 32]),
            snapshot,
            "  Purpose\r\n\r\n  stable\tcard  \r\n",
        )?;
        let equivalent = NormalizedSemanticCard::normalize_v1(
            SemanticCardId::from_bytes([3; 32]),
            snapshot,
            "Purpose\n\n\nstable card",
        )?;

        assert_eq!(first.body(), "Purpose\n\nstable card");
        assert_eq!(first.body_hash(), equivalent.body_hash());
        assert!(!format!("{first:?}").contains(first.body()));
        Ok(())
    }

    #[test]
    fn card_normalization_rejects_empty_control_and_oversized_input() {
        let snapshot = SnapshotId::from_bytes([4; 32]);
        let id = SemanticCardId::from_bytes([5; 32]);
        assert_eq!(
            NormalizedSemanticCard::normalize_v1(id, snapshot, " \n\t"),
            Err(SemanticCardNormalizationError::EmptyBody)
        );
        assert_eq!(
            NormalizedSemanticCard::normalize_v1(id, snapshot, "unsafe\0body"),
            Err(SemanticCardNormalizationError::UnsupportedControlCharacter)
        );
        assert!(matches!(
            NormalizedSemanticCard::normalize_v1(id, snapshot, &"x".repeat(65_537)),
            Err(SemanticCardNormalizationError::RawBodyTooLarge { .. })
        ));
    }

    #[test]
    fn card_batch_rejects_cross_snapshot_and_duplicate_logical_ids()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = SnapshotId::from_bytes([6; 32]);
        let id = SemanticCardId::from_bytes([7; 32]);
        let first = NormalizedSemanticCard::normalize_v1(id, snapshot, "first")?;
        let duplicate = NormalizedSemanticCard::normalize_v1(id, snapshot, "second")?;
        assert_eq!(
            SemanticCardBatch::new(snapshot, vec![first, duplicate]),
            Err(SemanticCardBatchError::DuplicateCardId)
        );

        let other = NormalizedSemanticCard::normalize_v1(
            SemanticCardId::from_bytes([8; 32]),
            SnapshotId::from_bytes([9; 32]),
            "other",
        )?;
        assert_eq!(
            SemanticCardBatch::new(snapshot, vec![other]),
            Err(SemanticCardBatchError::SnapshotMismatch)
        );
        Ok(())
    }
}
