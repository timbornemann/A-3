use super::{IntakeWindowByteLimit, IntakeWindowSelectionFailure, select_intake_window};
use a3_domain::{
    GoalContractReference, GoalContractTimestamp, GoalIntakeReview, GoalIntakeReviewBatchProposal,
    GoalIntakeReviewError, GoalOriginalRange, GoalOriginalRequest, MAX_GOAL_INTAKE_REVIEW_BATCHES,
    MAX_GOAL_ORIGINAL_PART_BYTES, PendingGoalIntake, TaskId,
};
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn pending(text: &str, captured_at: u64) -> TestResult<PendingGoalIntake> {
    let original = GoalOriginalRequest::try_from_bytes(
        GoalContractReference::initial(TaskId::from_bytes([31; 32])),
        text.as_bytes().to_vec(),
    )?;
    Ok(PendingGoalIntake::new(
        original,
        GoalContractTimestamp::from_unix_millis(captured_at)?,
    )?)
}

fn proposal(range: GoalOriginalRange) -> TestResult<GoalIntakeReviewBatchProposal> {
    Ok(GoalIntakeReviewBatchProposal::new(
        range,
        Vec::new(),
        Vec::new(),
    )?)
}

fn selected_text(pending: &PendingGoalIntake, cap: usize) -> TestResult<&str> {
    let window = select_intake_window(pending, None, IntakeWindowByteLimit::new(cap)?)?
        .ok_or("missing first window")?;
    Ok(pending.original().resolve(window)?)
}

#[test]
fn first_gap_is_independent_of_review_append_order() -> TestResult {
    let pending = pending("a b c d", 1)?;
    let original = pending.original();
    let mut review = GoalIntakeReview::begin(&pending, proposal(original.range(4, 5)?)?)?;
    review.append(&pending, proposal(original.range(2, 4)?)?)?;
    let limit = IntakeWindowByteLimit::new(2048)?;

    let first = select_intake_window(&pending, Some(&review), limit)?.ok_or("first gap")?;
    assert_eq!(original.resolve(first)?, "a ");
    review.append(&pending, proposal(first)?)?;

    let last = select_intake_window(&pending, Some(&review), limit)?.ok_or("tail gap")?;
    assert_eq!(original.resolve(last)?, " d");
    review.append(&pending, proposal(last)?)?;
    assert_eq!(select_intake_window(&pending, Some(&review), limit)?, None);
    assert!(review.is_fully_reviewed(&pending)?);
    Ok(())
}

#[test]
fn prefers_paragraph_and_list_boundaries_over_partial_continuation() -> TestResult {
    let paragraph = "First paragraph\r\n\r\n";
    let text = format!("{paragraph}Second paragraph\r\nIt continues here\r\n");
    let paragraph_input = pending(&text, 1)?;
    assert_eq!(
        selected_text(
            &paragraph_input,
            paragraph.len() + "Second paragraph\r\nIt ".len()
        )?,
        paragraph
    );

    let first = "Header\n  - First bullet\n    continued\n";
    for marker in ["- ", "2. ", "2) "] {
        let text = format!("{first}  {marker}Second bullet starts here\n");
        let list = pending(&text, 1)?;
        assert_eq!(selected_text(&list, first.len() + 10)?, first);
    }
    Ok(())
}

#[test]
fn utf8_crlf_and_whitespace_are_never_silently_cut_or_trimmed() -> TestResult {
    let unicode = pending("\u{1f680}\r\n\u{20ac}\n", 1)?;
    assert_eq!(
        select_intake_window(&unicode, None, IntakeWindowByteLimit::new(3)?),
        Err(IntakeWindowSelectionFailure::WindowTooSmall)
    );
    assert_eq!(selected_text(&unicode, 5)?, "\u{1f680}");
    assert_eq!(selected_text(&unicode, 6)?, "\u{1f680}\r\n");

    let crlf = pending("\r\nmore", 1)?;
    assert_eq!(
        select_intake_window(&crlf, None, IntakeWindowByteLimit::new(1)?),
        Err(IntakeWindowSelectionFailure::WindowTooSmall)
    );
    // A historical window may already have split CRLF. Its exact gap still belongs to Intake.
    let review = GoalIntakeReview::begin(&crlf, proposal(crlf.original().range(1, 2)?)?)?;
    let gap = select_intake_window(&crlf, Some(&review), IntakeWindowByteLimit::new(1)?)?
        .ok_or("historical CR gap")?;
    assert_eq!(crlf.original().resolve(gap)?, "\r");

    let whitespace = pending(" \r\n \r\n", 1)?;
    assert_eq!(selected_text(&whitespace, 3)?, " \r\n");
    Ok(())
}

#[test]
fn falls_back_to_complete_lines_or_the_longest_exact_prefix() -> TestResult {
    let multiline = pending("line one\nline two\n", 1)?;
    assert_eq!(selected_text(&multiline, 12)?, "line one\n");
    let long_line = pending(" \u{20ac}abcdef", 1)?;
    assert_eq!(selected_text(&long_line, 5)?, " \u{20ac}a");
    Ok(())
}

#[test]
fn all_original_bytes_and_late_constraints_remain_reachable() -> TestResult {
    let mut text = "- Requirement with examples and continued evidence.\r\n".repeat(350);
    let late = "R29: Preserve USER_NOTES.md exactly, including \u{1f680}.\r\n\t ";
    assert!(text.len() > 16 * 1024);
    text.push_str(late);

    let pending = pending(&text, 1)?;
    let limit = IntakeWindowByteLimit::new(1024)?;
    let mut review: Option<GoalIntakeReview> = None;
    let mut rebuilt = String::new();
    let mut saw_late_constraint = false;
    while let Some(window) = select_intake_window(&pending, review.as_ref(), limit)? {
        let piece = pending.original().resolve(window)?;
        assert!(!piece.is_empty() && piece.len() <= limit.get());
        assert_eq!(window.start_byte() as usize, rebuilt.len());
        saw_late_constraint |= piece.contains("Preserve USER_NOTES.md exactly");
        rebuilt.push_str(piece);
        match review.as_mut() {
            Some(review) => review.append(&pending, proposal(window)?)?,
            None => review = Some(GoalIntakeReview::begin(&pending, proposal(window)?)?),
        }
    }
    assert!(saw_late_constraint);
    assert_eq!(rebuilt.as_bytes(), pending.original().as_bytes());
    assert!(
        review
            .ok_or("review history")?
            .is_fully_reviewed(&pending)?
    );
    Ok(())
}

#[test]
fn rejects_invalid_cap_foreign_capture_and_incomplete_history_at_capacity() -> TestResult {
    assert_eq!(
        IntakeWindowByteLimit::new(0),
        Err(IntakeWindowSelectionFailure::InvalidByteLimit)
    );
    assert_eq!(
        IntakeWindowByteLimit::new(MAX_GOAL_ORIGINAL_PART_BYTES + 1),
        Err(IntakeWindowSelectionFailure::InvalidByteLimit)
    );
    let text = "a".repeat(MAX_GOAL_INTAKE_REVIEW_BATCHES + 1);
    let capture = pending(&text, 1)?;
    let other = pending(&text, 2)?;
    let first = proposal(capture.original().range(0, 1)?)?;
    let mut review = GoalIntakeReview::begin(&capture, first)?;
    let limit = IntakeWindowByteLimit::new(MAX_GOAL_ORIGINAL_PART_BYTES)?;
    assert_eq!(
        select_intake_window(&other, Some(&review), limit),
        Err(IntakeWindowSelectionFailure::Review(
            GoalIntakeReviewError::CaptureMismatch
        ))
    );
    for i in 1..MAX_GOAL_INTAKE_REVIEW_BATCHES {
        review.append(&capture, proposal(capture.original().range(i, i + 1)?)?)?;
    }
    assert_eq!(
        select_intake_window(&capture, Some(&review), limit),
        Err(IntakeWindowSelectionFailure::Review(
            GoalIntakeReviewError::HistoryLimit
        ))
    );
    Ok(())
}
