use super::GoalContractReference;
use std::error::Error;
use std::fmt;

/// Maximum exact UTF-8 bytes retained for one original goal request.
pub const MAX_GOAL_ORIGINAL_BYTES: usize = 65_536;
/// Maximum exact UTF-8 bytes in one deterministic original-request part.
pub const MAX_GOAL_ORIGINAL_PART_BYTES: usize = 2_048;
/// Maximum parts needed when UTF-8 boundaries prevent 32 full-sized parts.
pub const MAX_GOAL_ORIGINAL_PARTS: usize = 33;

/// Domain-separated digest of the exact original bytes, before any normalization.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalOriginalDigest([u8; 32]);

impl GoalOriginalDigest {
    /// Reconstructs a stored digest; the corresponding bytes still require validation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical digest bytes for persistence and equality checks.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for GoalOriginalDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GoalOriginalDigest([REDACTED])")
    }
}

/// Nonempty UTF-8 range bound to one immutable original and Goal Contract revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GoalOriginalRange {
    goal: GoalContractReference,
    digest: GoalOriginalDigest,
    start_byte: u32,
    end_byte: u32,
}

impl GoalOriginalRange {
    /// Returns the validated task-and-revision anchor.
    #[must_use]
    pub const fn goal(self) -> GoalContractReference {
        self.goal
    }

    /// Returns the digest of the complete exact original, not only this range.
    #[must_use]
    pub const fn digest(self) -> GoalOriginalDigest {
        self.digest
    }

    /// Returns the inclusive UTF-8 byte offset in the complete original.
    #[must_use]
    pub const fn start_byte(self) -> u32 {
        self.start_byte
    }

    /// Returns the exclusive UTF-8 byte offset in the complete original.
    #[must_use]
    pub const fn end_byte(self) -> u32 {
        self.end_byte
    }
}

/// Exact, immutable original request retained by the existing Goal store.
#[derive(Clone, PartialEq, Eq)]
pub struct GoalOriginalRequest {
    goal: GoalContractReference,
    digest: GoalOriginalDigest,
    text: String,
    parts: Vec<GoalOriginalRange>,
}

impl GoalOriginalRequest {
    /// Validates exact UTF-8 bytes without trimming, filtering or normalizing them.
    pub fn try_from_bytes(
        goal: GoalContractReference,
        bytes: Vec<u8>,
    ) -> Result<Self, GoalOriginalError> {
        if bytes.is_empty() {
            return Err(GoalOriginalError::Empty);
        }
        if bytes.len() > MAX_GOAL_ORIGINAL_BYTES {
            return Err(GoalOriginalError::TooLarge {
                actual_bytes: bytes.len(),
            });
        }
        let text = String::from_utf8(bytes).map_err(|_| GoalOriginalError::InvalidUtf8)?;
        let mut hasher = blake3::Hasher::new_derive_key("a3.goal-original-request.v1");
        hasher.update(&(text.len() as u64).to_le_bytes());
        hasher.update(text.as_bytes());
        let digest = GoalOriginalDigest(*hasher.finalize().as_bytes());
        let mut request = Self {
            goal,
            digest,
            text,
            parts: Vec::new(),
        };
        let mut start = 0;
        while start < request.text.len() {
            let mut end = (start + MAX_GOAL_ORIGINAL_PART_BYTES).min(request.text.len());
            while !request.text.is_char_boundary(end) {
                end -= 1;
            }
            if end <= start || request.parts.len() == MAX_GOAL_ORIGINAL_PARTS {
                return Err(GoalOriginalError::PartLimitExceeded);
            }
            request.parts.push(request.range(start, end)?);
            start = end;
        }
        Ok(request)
    }

    /// Reconstructs stored bytes and rejects a stored digest that does not match them.
    pub fn reconstruct(
        goal: GoalContractReference,
        bytes: Vec<u8>,
        expected_digest: GoalOriginalDigest,
    ) -> Result<Self, GoalOriginalError> {
        let original = Self::try_from_bytes(goal, bytes)?;
        if original.digest != expected_digest {
            return Err(GoalOriginalError::DigestMismatch);
        }
        Ok(original)
    }

    /// Returns the validated Goal Contract revision that owns this original.
    #[must_use]
    pub const fn goal(&self) -> GoalContractReference {
        self.goal
    }

    /// Returns the exact original byte digest, independent of task/revision identity.
    #[must_use]
    pub const fn digest(&self) -> GoalOriginalDigest {
        self.digest
    }

    /// Returns the unchanged original UTF-8 bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    /// Returns the unchanged original UTF-8 text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns contiguous ordered parts, each at most 2048 bytes with UTF-8 boundaries.
    #[must_use]
    pub fn parts(&self) -> &[GoalOriginalRange] {
        &self.parts
    }

    /// Binds a nonempty UTF-8 range to this exact original and its Goal revision.
    pub fn range(
        &self,
        start_byte: usize,
        end_byte: usize,
    ) -> Result<GoalOriginalRange, GoalOriginalError> {
        self.restore_range(self.goal, self.digest, start_byte, end_byte)
    }

    /// Restores a stored range only after validating its owner, digest and UTF-8 bounds.
    pub fn restore_range(
        &self,
        goal: GoalContractReference,
        digest: GoalOriginalDigest,
        start_byte: usize,
        end_byte: usize,
    ) -> Result<GoalOriginalRange, GoalOriginalError> {
        if goal != self.goal {
            return Err(GoalOriginalError::GoalMismatch);
        }
        if digest != self.digest {
            return Err(GoalOriginalError::DigestMismatch);
        }
        if start_byte > end_byte {
            return Err(GoalOriginalError::InvertedRange);
        }
        if start_byte == end_byte {
            return Err(GoalOriginalError::EmptyRange);
        }
        if end_byte > self.text.len() {
            return Err(GoalOriginalError::RangeOutOfBounds);
        }
        if !self.text.is_char_boundary(start_byte) || !self.text.is_char_boundary(end_byte) {
            return Err(GoalOriginalError::InvalidBoundary);
        }
        Ok(GoalOriginalRange {
            goal,
            digest,
            start_byte: u32::try_from(start_byte)
                .map_err(|_| GoalOriginalError::RangeOutOfBounds)?,
            end_byte: u32::try_from(end_byte).map_err(|_| GoalOriginalError::RangeOutOfBounds)?,
        })
    }

    /// Resolves only a current range that belongs to this exact original and revision.
    pub fn resolve(&self, range: GoalOriginalRange) -> Result<&str, GoalOriginalError> {
        let checked = self.restore_range(
            range.goal,
            range.digest,
            range.start_byte as usize,
            range.end_byte as usize,
        )?;
        self.text
            .get(checked.start_byte as usize..checked.end_byte as usize)
            .ok_or(GoalOriginalError::InvalidBoundary)
    }
}

impl fmt::Debug for GoalOriginalRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoalOriginalRequest")
            .field("goal", &self.goal)
            .field("digest", &self.digest)
            .field("bytes", &self.text.len())
            .field("parts", &self.parts.len())
            .finish_non_exhaustive()
    }
}

/// Content-free rejection of original bytes or a bound range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalOriginalError {
    /// The original contained no bytes.
    Empty,
    /// The original exceeded the fixed byte boundary before UTF-8 parsing.
    TooLarge {
        /// Observed original length, never its contents.
        actual_bytes: usize,
    },
    /// Original bytes were not valid UTF-8.
    InvalidUtf8,
    /// Partitioning could not retain all bytes within its fixed bounds.
    PartLimitExceeded,
    /// The referenced task or immutable Goal revision did not match.
    GoalMismatch,
    /// The referenced digest did not match the exact original bytes.
    DigestMismatch,
    /// A range had no original bytes.
    EmptyRange,
    /// A range start followed its end.
    InvertedRange,
    /// A range exceeded the exact original.
    RangeOutOfBounds,
    /// A range split a UTF-8 code point.
    InvalidBoundary,
}

impl fmt::Display for GoalOriginalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("goal original request is empty"),
            Self::TooLarge { actual_bytes } => write!(
                formatter,
                "goal original request has {actual_bytes} bytes; maximum is {MAX_GOAL_ORIGINAL_BYTES}"
            ),
            Self::InvalidUtf8 => formatter.write_str("goal original request is not UTF-8"),
            Self::PartLimitExceeded => formatter.write_str("goal original part bound exceeded"),
            Self::GoalMismatch => formatter.write_str("goal original revision does not match"),
            Self::DigestMismatch => formatter.write_str("goal original digest does not match"),
            Self::EmptyRange => formatter.write_str("goal original range is empty"),
            Self::InvertedRange => formatter.write_str("goal original range is inverted"),
            Self::RangeOutOfBounds => {
                formatter.write_str("goal original range is outside the original")
            }
            Self::InvalidBoundary => formatter.write_str("goal original range splits UTF-8"),
        }
    }
}

impl Error for GoalOriginalError {}

#[cfg(test)]
mod tests {
    use super::{
        GoalOriginalDigest, GoalOriginalError, GoalOriginalRequest, MAX_GOAL_ORIGINAL_BYTES,
        MAX_GOAL_ORIGINAL_PART_BYTES, MAX_GOAL_ORIGINAL_PARTS,
    };
    use crate::{
        AcceptanceCriterion, AcceptanceCriterionId, AcceptanceCriterionStatement, GoalContract,
        GoalContractDraft, GoalContractReference, GoalContractTimestamp, GoalObjective,
        GoalRevisionReason, SuccessVerification, TaskId,
    };
    use std::error::Error;

    fn draft(objective: &str) -> Result<GoalContractDraft, Box<dyn Error>> {
        Ok(GoalContractDraft::new(
            GoalObjective::try_from_string(objective.to_owned())?,
            vec![AcceptanceCriterion::new(
                AcceptanceCriterionId::from_bytes([2; 32]),
                AcceptanceCriterionStatement::try_from_string(
                    "Requested behavior holds".to_owned(),
                )?,
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            SuccessVerification::try_from_string("Observe the requested behavior".to_owned())?,
        )?)
    }

    fn goal(task: u8, revised: bool) -> Result<GoalContractReference, Box<dyn Error>> {
        let initial = GoalContract::initial(
            TaskId::from_bytes([task; 32]),
            draft("Initial required outcome")?,
            GoalContractTimestamp::from_unix_millis(1)?,
        );
        if revised {
            Ok(initial
                .revise(
                    draft("Revised required outcome")?,
                    GoalRevisionReason::try_from_string("User changed the outcome".to_owned())?,
                    GoalContractTimestamp::from_unix_millis(2)?,
                )?
                .reference())
        } else {
            Ok(initial.reference())
        }
    }

    fn assert_parts(original: &GoalOriginalRequest) -> Result<(), Box<dyn Error>> {
        let mut rebuilt = Vec::new();
        let mut next = 0;
        assert!(!original.parts().is_empty());
        assert!(original.parts().len() <= MAX_GOAL_ORIGINAL_PARTS);
        for part in original.parts() {
            assert_eq!(part.goal(), original.goal());
            assert_eq!(part.digest(), original.digest());
            assert_eq!(part.start_byte() as usize, next);
            assert!(part.end_byte() > part.start_byte());
            assert!((part.end_byte() - part.start_byte()) as usize <= MAX_GOAL_ORIGINAL_PART_BYTES);
            let text = original.resolve(*part)?;
            rebuilt.extend_from_slice(text.as_bytes());
            next = part.end_byte() as usize;
        }
        assert_eq!(next, original.as_bytes().len());
        assert_eq!(rebuilt, original.as_bytes());
        Ok(())
    }

    #[test]
    fn ascii_exact_64k_is_retained_in_32_contiguous_parts() -> Result<(), Box<dyn Error>> {
        let bytes = vec![b'x'; MAX_GOAL_ORIGINAL_BYTES];
        let original = GoalOriginalRequest::try_from_bytes(goal(1, false)?, bytes.clone())?;
        assert_eq!(original.as_bytes(), bytes);
        assert_eq!(original.parts().len(), 32);
        assert_parts(&original)?;
        Ok(())
    }

    #[test]
    fn unicode_exact_64k_is_not_rejected_when_it_needs_a_33rd_part() -> Result<(), Box<dyn Error>> {
        let mut text = "€".repeat(MAX_GOAL_ORIGINAL_BYTES / 3);
        text.push('x');
        assert_eq!(text.len(), MAX_GOAL_ORIGINAL_BYTES);
        let original =
            GoalOriginalRequest::try_from_bytes(goal(1, false)?, text.as_bytes().to_vec())?;
        assert_eq!(original.as_str(), text);
        assert_eq!(original.parts().len(), 33);
        assert_parts(&original)?;
        Ok(())
    }

    #[test]
    fn all_utf8_widths_and_boundary_offsets_keep_every_original_byte() -> Result<(), Box<dyn Error>>
    {
        let reference = goal(1, false)?;
        for character in ['é', '€', '🚀'] {
            for padding in 0..8 {
                let mut text = "x".repeat(padding);
                text.push_str(
                    &character
                        .to_string()
                        .repeat((MAX_GOAL_ORIGINAL_BYTES - padding) / character.len_utf8()),
                );
                text.push_str(&"z".repeat(MAX_GOAL_ORIGINAL_BYTES - text.len()));
                let original =
                    GoalOriginalRequest::try_from_bytes(reference, text.as_bytes().to_vec())?;
                assert_eq!(original.as_str(), text);
                assert_parts(&original)?;
            }
        }
        Ok(())
    }

    #[test]
    fn whitespace_crlf_bom_and_part_boundaries_are_not_normalized() -> Result<(), Box<dyn Error>> {
        let text = format!(
            "\u{feff}  \r\n{}\r\n\tR01: Grüße 🚀\r\nR02: 日本語\r\n  ",
            "a".repeat(2_040)
        );
        let original =
            GoalOriginalRequest::try_from_bytes(goal(1, false)?, text.as_bytes().to_vec())?;
        assert_eq!(original.as_str(), text);
        assert!(original.as_str().starts_with("\u{feff}  \r\n"));
        assert!(original.as_str().ends_with("\r\n  "));
        assert_parts(&original)?;
        Ok(())
    }

    #[test]
    fn empty_oversized_and_invalid_utf8_are_rejected_without_truncation()
    -> Result<(), Box<dyn Error>> {
        let reference = goal(1, false)?;
        assert_eq!(
            GoalOriginalRequest::try_from_bytes(reference, Vec::new()),
            Err(GoalOriginalError::Empty)
        );
        assert_eq!(
            GoalOriginalRequest::try_from_bytes(reference, vec![b'x'; MAX_GOAL_ORIGINAL_BYTES + 1]),
            Err(GoalOriginalError::TooLarge {
                actual_bytes: MAX_GOAL_ORIGINAL_BYTES + 1
            })
        );
        assert_eq!(
            GoalOriginalRequest::try_from_bytes(reference, vec![0xf0, 0x9f, 0x9a]),
            Err(GoalOriginalError::InvalidUtf8)
        );
        assert_eq!(
            GoalOriginalRequest::try_from_bytes(reference, vec![0xc0, 0xaf]),
            Err(GoalOriginalError::InvalidUtf8)
        );
        assert_eq!(
            GoalOriginalRequest::try_from_bytes(reference, b" \r\n\t".to_vec())?.as_bytes(),
            b" \r\n\t"
        );
        Ok(())
    }

    #[test]
    fn range_requires_nonempty_current_utf8_boundaries_without_overflow()
    -> Result<(), Box<dyn Error>> {
        let original =
            GoalOriginalRequest::try_from_bytes(goal(1, false)?, "a€z".as_bytes().to_vec())?;
        let range = original.range(1, 4)?;
        assert_eq!(range.start_byte(), 1);
        assert_eq!(range.end_byte(), 4);
        assert_eq!(original.resolve(range)?, "€");
        assert_eq!(original.resolve(original.range(0, 5)?)?, "a€z");
        for (start, end, error) in [
            (1, 1, GoalOriginalError::EmptyRange),
            (4, 1, GoalOriginalError::InvertedRange),
            (0, 6, GoalOriginalError::RangeOutOfBounds),
            (
                usize::MAX - 1,
                usize::MAX,
                GoalOriginalError::RangeOutOfBounds,
            ),
            (2, 4, GoalOriginalError::InvalidBoundary),
            (1, 3, GoalOriginalError::InvalidBoundary),
        ] {
            assert_eq!(original.range(start, end), Err(error));
        }
        Ok(())
    }

    #[test]
    fn foreign_task_or_goal_revision_is_rejected_even_for_identical_bytes()
    -> Result<(), Box<dyn Error>> {
        let bytes = b"Original request".to_vec();
        let first = GoalOriginalRequest::try_from_bytes(goal(1, false)?, bytes.clone())?;
        let foreign = GoalOriginalRequest::try_from_bytes(goal(2, false)?, bytes.clone())?;
        let revised = GoalOriginalRequest::try_from_bytes(goal(1, true)?, bytes)?;
        let range = first.range(0, 8)?;
        assert_eq!(first.digest(), foreign.digest());
        assert_eq!(first.digest(), revised.digest());
        for other in [&foreign, &revised] {
            assert_eq!(other.resolve(range), Err(GoalOriginalError::GoalMismatch));
            assert_eq!(
                first.restore_range(other.goal(), first.digest(), 0, 8),
                Err(GoalOriginalError::GoalMismatch)
            );
        }
        Ok(())
    }

    #[test]
    fn stale_or_tampered_digest_cannot_restore_or_resolve_a_reference() -> Result<(), Box<dyn Error>>
    {
        let reference = goal(1, false)?;
        let original =
            GoalOriginalRequest::try_from_bytes(reference, b"Original request".to_vec())?;
        let range = original.range(0, 8)?;
        let changed =
            GoalOriginalRequest::try_from_bytes(reference, b"Different request".to_vec())?;
        assert_ne!(original.digest(), changed.digest());
        assert_eq!(
            changed.resolve(range),
            Err(GoalOriginalError::DigestMismatch)
        );
        let forged = GoalOriginalDigest::from_bytes([0; 32]);
        assert_eq!(
            original.restore_range(reference, forged, 0, 8),
            Err(GoalOriginalError::DigestMismatch)
        );
        assert_eq!(
            GoalOriginalRequest::reconstruct(reference, original.as_bytes().to_vec(), forged),
            Err(GoalOriginalError::DigestMismatch)
        );
        let restored = GoalOriginalRequest::reconstruct(
            reference,
            original.as_bytes().to_vec(),
            GoalOriginalDigest::from_bytes(*original.digest().as_bytes()),
        )?;
        assert_eq!(restored, original);
        assert_eq!(restored.resolve(range)?, "Original");
        Ok(())
    }

    #[test]
    fn partition_and_digest_are_deterministic_and_byte_exact() -> Result<(), Box<dyn Error>> {
        let reference = goal(1, false)?;
        let text = "R01: Grüße\r\nR02: 日本語🚀\r\n".repeat(200);
        let first = GoalOriginalRequest::try_from_bytes(reference, text.as_bytes().to_vec())?;
        let second = GoalOriginalRequest::try_from_bytes(reference, text.as_bytes().to_vec())?;
        assert_eq!(first, second);
        let normalized = GoalOriginalRequest::try_from_bytes(
            reference,
            text.replace("\r\n", "\n").into_bytes(),
        )?;
        assert_ne!(first.digest(), normalized.digest());
        assert_parts(&first)?;
        Ok(())
    }

    #[test]
    fn debug_and_error_messages_never_include_original_text() -> Result<(), Box<dyn Error>> {
        let text = "private-original-request-marker";
        let original =
            GoalOriginalRequest::try_from_bytes(goal(1, false)?, text.as_bytes().to_vec())?;
        let range = original.range(0, text.len())?;
        for debug in [
            format!("{original:?}"),
            format!("{range:?}"),
            format!("{:?}", original.digest()),
        ] {
            assert!(!debug.contains(text));
        }
        for error in [
            GoalOriginalError::Empty,
            GoalOriginalError::TooLarge {
                actual_bytes: 65_537,
            },
            GoalOriginalError::InvalidUtf8,
            GoalOriginalError::PartLimitExceeded,
            GoalOriginalError::GoalMismatch,
            GoalOriginalError::DigestMismatch,
            GoalOriginalError::EmptyRange,
            GoalOriginalError::InvertedRange,
            GoalOriginalError::RangeOutOfBounds,
            GoalOriginalError::InvalidBoundary,
        ] {
            assert!(!format!("{error} {error:?}").contains(text));
        }
        Ok(())
    }
}
