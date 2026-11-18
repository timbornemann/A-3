//! Selects exact Pending original ranges for bounded Intake interpretation.

use a3_domain::{
    GoalIntakeReview, GoalIntakeReviewError, GoalOriginalError, GoalOriginalRange,
    MAX_GOAL_INTAKE_REVIEW_BATCHES, MAX_GOAL_ORIGINAL_PART_BYTES, PendingGoalIntake,
};
use std::{error::Error, fmt, ops::Range};

/// Source-byte ceiling for one selection. The actual model wire has its own fit check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntakeWindowByteLimit(usize);

impl IntakeWindowByteLimit {
    /// Accepts a positive ceiling up to the existing Intake window bound.
    pub fn new(bytes: usize) -> Result<Self, IntakeWindowSelectionFailure> {
        if !(1..=MAX_GOAL_ORIGINAL_PART_BYTES).contains(&bytes) {
            return Err(IntakeWindowSelectionFailure::InvalidByteLimit);
        }
        Ok(Self(bytes))
    }

    /// Returns the source-byte ceiling.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Selects a prefix of the first byte gap in the bound, committed review history.
///
/// Within the limit, prefer a paragraph or list boundary, then a complete line.
/// An oversized line can continue in the next window. The selection preserves
/// whitespace and line endings; it does not attribute them or a continuation
/// to a requirement. A previously reviewed range can already split a line or CRLF.
/// An empty result means every byte was reviewed, independently of open questions.
///
/// This is selection only. Before reserving any attempt, the caller can choose a
/// smaller ceiling and use the existing exact provider wire-fit check. Attempted
/// windows are governed by the durable Intake reservation, even if no review batch
/// was committed. A rejected reservation must not cause silent retries or gaps.
pub fn select_intake_window(
    pending: &PendingGoalIntake,
    review: Option<&GoalIntakeReview>,
    limit: IntakeWindowByteLimit,
) -> Result<Option<GoalOriginalRange>, IntakeWindowSelectionFailure> {
    if let Some(review) = review {
        review
            .validate_capture(pending)
            .map_err(IntakeWindowSelectionFailure::Review)?;
    }
    let original = pending.original();
    let text = original.as_str();
    let gap = first_unreviewed_gap(review, text.len());
    if gap.is_empty() {
        return Ok(None);
    }
    if review.is_some_and(|review| review.batches().len() >= MAX_GOAL_INTAKE_REVIEW_BATCHES) {
        return Err(IntakeWindowSelectionFailure::Review(
            GoalIntakeReviewError::HistoryLimit,
        ));
    }

    let start = gap.start;
    let mut end = (start + limit.get()).min(gap.end);
    if end < gap.end {
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end > start && text.as_bytes()[end - 1] == b'\r' && text.as_bytes()[end] == b'\n' {
            end -= 1;
        }
        if end == start {
            return Err(IntakeWindowSelectionFailure::WindowTooSmall);
        }
        end = start + preferred_prefix_bytes(&text[start..end]);
    }
    original
        .range(start, end)
        .map(Some)
        .map_err(IntakeWindowSelectionFailure::Original)
}

fn first_unreviewed_gap(review: Option<&GoalIntakeReview>, original_bytes: usize) -> Range<usize> {
    let Some(review) = review else {
        return 0..original_bytes;
    };
    let mut windows = review
        .batches()
        .iter()
        .map(|batch| batch.window())
        .collect::<Vec<_>>();
    windows.sort_unstable_by_key(|window| window.start_byte());

    let mut cursor = 0;
    for window in windows {
        let start = window.start_byte() as usize;
        if cursor < start {
            return cursor..start;
        }
        cursor = window.end_byte() as usize;
    }
    cursor..original_bytes
}

fn preferred_prefix_bytes(prefix: &str) -> usize {
    let mut offset = 0;
    let mut has_content = false;
    let mut natural = None;
    let mut complete_line = None;
    for line in prefix.split_inclusive('\n') {
        if has_content && starts_list_item(line) {
            natural = Some(offset);
        }
        let blank = line.trim().is_empty();
        has_content |= !blank;
        offset += line.len();
        if has_content && line.ends_with('\n') {
            complete_line = Some(offset);
            if blank {
                natural = Some(offset);
            }
        }
    }
    natural.or(complete_line).unwrap_or(prefix.len())
}

fn starts_list_item(line: &str) -> bool {
    let bytes = line.trim_start_matches([' ', '\t']).as_bytes();
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    matches!(bytes, [b'-' | b'+' | b'*', b' ' | b'\t', ..])
        || (digits > 0 && matches!(&bytes[digits..], [b'.' | b')', b' ' | b'\t', ..]))
}

/// Content-free failures in finding the next eligible original byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakeWindowSelectionFailure {
    /// The caller's source-byte ceiling is outside the Intake limit.
    InvalidByteLimit,
    /// The first UTF-8 character or CRLF of the gap cannot fit.
    WindowTooSmall,
    /// The committed review belongs to another capture or is at capacity.
    Review(GoalIntakeReviewError),
    /// The selected range could not be bound to the original.
    Original(GoalOriginalError),
}

impl fmt::Display for IntakeWindowSelectionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidByteLimit => f.write_str("intake window byte limit is invalid"),
            Self::WindowTooSmall => {
                f.write_str("intake window is too small for the next text unit")
            }
            Self::Review(error) => write!(f, "intake window review: {error}"),
            Self::Original(error) => write!(f, "intake window source: {error}"),
        }
    }
}

impl Error for IntakeWindowSelectionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Review(error) => Some(error),
            Self::Original(error) => Some(error),
            Self::InvalidByteLimit | Self::WindowTooSmall => None,
        }
    }
}

#[cfg(test)]
mod tests;
