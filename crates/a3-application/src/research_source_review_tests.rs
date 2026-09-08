use super::*;
use a3_domain::{
    AskResearchSourceId, ContentHash, FileRevision, RepositoryPath, SourcePosition, SourceRange,
};
use std::error::Error;

fn revision() -> Result<FileRevision, Box<dyn Error>> {
    Ok(FileRevision::new(
        RepositoryPath::try_from_bytes(b"fixture.py".to_vec())?,
        ContentHash::from_bytes([1; 32]),
    ))
}
fn window<'a>(
    revision: &'a FileRevision,
    text: &'a str,
) -> Result<ResearchEvidenceWindow<'a>, Box<dyn Error>> {
    let start = SourcePosition::new(8, 0);
    Ok(ResearchEvidenceWindow {
        anchor: Some(crate::ResearchEvidenceAnchorId::new(2)?),
        ordinal: 1,
        source_id: AskResearchSourceId::from_bytes([2; 32]),
        revision,
        range: SourceRange::new(
            100,
            100 + text.len(),
            start,
            crate::research_work_admission::position_after(start, text).ok_or("position")?,
        )?,
        text,
    })
}
fn wire(interpretation: &str, quotes: &[&str]) -> String {
    serde_json::json!({"schema_version":2,"interpretation":interpretation,"quotes":quotes})
        .to_string()
}

#[test]
fn source_review_binds_original_utf8_spans_not_truth_or_turn_labels() -> Result<(), Box<dyn Error>>
{
    let revision = revision()?;
    let mut w = window(&revision, "# ä\r\nreturn ('json', tasks)\r\n")?;
    // Deliberately false interpretation + real quote: only provenance is admitted.
    let review = ResearchSourceReview::decode(
        &wire(
            "Saves to disk",
            &["return ('json', tasks)", "return ('json', tasks)"],
        ),
        &w,
    )?;
    assert_eq!(review.quotes().len(), 1);
    assert_eq!(review.quotes()[0].range.start_byte(), 106);
    assert_eq!(
        review.quotes()[0].range.start_position(),
        SourcePosition::new(9, 0)
    );
    assert_eq!(review.interpretation(), "Saves to disk");
    w.anchor = Some(crate::ResearchEvidenceAnchorId::new(1)?);
    w.ordinal = 7;
    assert!(
        review.matches(&w),
        "Core rebinds the current label, never a cached alias"
    );
    w.source_id = AskResearchSourceId::from_bytes([3; 32]);
    assert!(!review.matches(&w));
    let changed = FileRevision::new(revision.path().clone(), ContentHash::from_bytes([9; 32]));
    assert!(!review.matches(&window(&changed, w.text)?));
    assert!(!review.matches(&window(&revision, "# ä\r\n")?));
    let debug = format!("{review:?}");
    assert!(!debug.contains("disk") && !debug.contains("fixture.py") && !debug.contains("return"));
    Ok(())
}

#[test]
fn source_review_strict_wire_and_actual_byte_bounds() -> Result<(), Box<dyn Error>> {
    let revision = revision()?;
    let w = window(&revision, "unique original")?;
    assert!(ResearchSourceReview::decode(&wire(&"ä".repeat(96), &[w.text]), &w).is_ok());
    assert!(
        ResearchSourceReview::decode(
            &wire("\"quoted\": {braces} [list] \\ backslash", &[w.text]),
            &w
        )
        .is_ok()
    );
    for raw in [
        wire(&"ä".repeat(97), &[w.text]),
        wire("\n", &[w.text]),
        wire("line\nline", &[w.text]),
        wire("valid", &[]),
        wire("valid", &[w.text; 5]),
        wire("valid", &[" "]),
        wire("valid", &["other original"]),
        wire("valid", &[w.text]).replace("\"schema_version\":2", "\"schema_version\":1"),
        wire("valid", &[w.text]).replacen('{', "{\"extra\":0,", 1),
        wire("valid", &[w.text]).replacen('{', "{\"schema_version\":1,", 1),
        wire("valid", &[w.text]).replacen('{', "{\"schema_versi\\u006fn\":1,", 1),
        format!("{}{}", wire("valid", &[w.text]), " ".repeat(4096)),
        "{\"schema_version\":1,\"interpretation\":true,\"quotes\":[\"unique original\"]}"
            .to_owned(),
    ] {
        assert!(
            ResearchSourceReview::decode(&raw, &w).is_err(),
            "must reject closed invalid case"
        );
    }
    let text = "ü".repeat(257);
    let unicode = window(&revision, &text)?;
    assert_eq!(
        ResearchSourceReview::decode(&wire("valid", &[&text]), &unicode).err(),
        Some(ResearchSourceReviewError::QuoteLimit {
            ordinal: 1,
            bytes: 514
        })
    );
    research_source_review_schema()?;
    Ok(())
}

#[test]
fn source_review_rejects_ambiguous_overlapping_quotes_and_false_ranges()
-> Result<(), Box<dyn Error>> {
    let revision = revision()?;
    for (text, quote) in [("same same", "same"), ("aaa", "aa"), ("äää", "ää")] {
        assert_eq!(
            ResearchSourceReview::decode(&wire("valid", &[quote]), &window(&revision, text)?).err(),
            Some(ResearchSourceReviewError::Ambiguous)
        );
    }
    let mut w = window(&revision, "original")?;
    w.range = SourceRange::new(
        100,
        107,
        SourcePosition::new(8, 0),
        SourcePosition::new(8, 7),
    )?;
    assert_eq!(
        ResearchSourceReview::decode(&wire("valid", &["original"]), &w).err(),
        Some(ResearchSourceReviewError::Original)
    );
    w.range = SourceRange::new(
        100,
        108,
        SourcePosition::new(8, 0),
        SourcePosition::new(9, 0),
    )?;
    assert_eq!(
        ResearchSourceReview::decode(&wire("valid", &["original"]), &w).err(),
        Some(ResearchSourceReviewError::Original)
    );
    Ok(())
}

#[test]
fn source_review_v2_admits_complete_260_and_512_byte_quotes_but_not_overflow()
-> Result<(), Box<dyn Error>> {
    let revision = revision()?;
    for text in ["a".repeat(260), "b".repeat(512), "ü".repeat(256)] {
        let w = window(&revision, &text)?;
        let review = ResearchSourceReview::decode(
            &wire(
                "Only provenance, not proof of this interpretation.",
                &[&text],
            ),
            &w,
        )?;
        assert_eq!(
            review.quotes()[0].range,
            w.range,
            "the complete original stays intact"
        );
    }
    let text = "c".repeat(513);
    assert_eq!(
        ResearchSourceReview::decode(&wire("valid", &[&text]), &window(&revision, &text)?).err(),
        Some(ResearchSourceReviewError::QuoteLimit {
            ordinal: 1,
            bytes: 513
        })
    );
    let valid = wire("valid", &["current original"]);
    assert_eq!(
        ResearchSourceReview::decode(
            &valid.replace("\"schema_version\":2", "\"schema_version\":1"),
            &window(&revision, "current original")?
        )
        .err(),
        Some(ResearchSourceReviewError::Version)
    );
    Ok(())
}

#[test]
fn source_review_repair_identifies_the_failing_field_without_rejected_content()
-> Result<(), Box<dyn Error>> {
    let revision = revision()?;
    let text = "private-looking-q9 ".repeat(30);
    let w = window(&revision, &text)?;
    let quote_error = ResearchSourceReview::decode(&wire("valid", &[&text]), &w)
        .err()
        .ok_or("must fail")?;
    assert_eq!(
        quote_error,
        ResearchSourceReviewError::QuoteLimit {
            ordinal: 1,
            bytes: text.len()
        }
    );
    assert!(quote_error.repair_hint().contains("quote 1"));
    assert!(quote_error.repair_hint().contains(&text.len().to_string()));
    assert!(!quote_error.repair_hint().contains("private-looking-q9"));
    let interpretation = ResearchSourceReview::decode(&wire(&"x".repeat(193), &["unique"]), &w)
        .err()
        .ok_or("must fail")?;
    assert_eq!(
        interpretation,
        ResearchSourceReviewError::InterpretationLimit { bytes: 193 }
    );
    assert!(
        interpretation
            .repair_hint()
            .starts_with("interpretation has 193")
    );
    for issue in [
        quote_error,
        interpretation,
        ResearchSourceReviewError::Shape,
        ResearchSourceReviewError::Version,
        ResearchSourceReviewError::Limit,
        ResearchSourceReviewError::InvalidInterpretation,
        ResearchSourceReviewError::QuoteCount { count: 0 },
        ResearchSourceReviewError::Original,
        ResearchSourceReviewError::Ambiguous,
    ] {
        assert!(
            issue.repair_hint().len() < 384,
            "room for the unchanged full contract within the 768-byte repair reserve"
        );
    }
    Ok(())
}
