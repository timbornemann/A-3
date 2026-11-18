//! Pure byte/grammar contracts; database authority and real wire accounting are native tests.
use super::*;
use a3_domain::{FileRevision, RepositoryPath};
type TestResult<T = ()> = Result<T, Box<dyn Error>>;
fn page(text: &str, more: bool) -> TestResult<AgentSourcePage> {
    let prefix = "outside\r\n";
    let full = format!("{prefix}{text}{}", if more { "after\r\n" } else { "" });
    let rows = u32::try_from(text.bytes().filter(|b| *b == b'\n').count())?;
    let tail = text
        .rsplit_once('\n')
        .map_or(text.len(), |(_, tail)| tail.len());
    let range = SourceRange::new(
        prefix.len(),
        prefix.len() + text.len(),
        SourcePosition::new(1, 0),
        SourcePosition::new(1 + rows, u32::try_from(tail)?),
    )?;
    Ok(AgentSourcePage::new(
        FileRevision::new(
            RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
            ContentHash::from_bytes(*blake3::hash(full.as_bytes()).as_bytes()),
        ),
        range,
        AgentFileStartLine::new(2)?,
        text.to_owned(),
        if more {
            Some(AgentFileStartLine::new(2 + rows)?)
        } else {
            None
        },
        more,
    )?)
}
#[test]
fn two_deterministic_halves_bind_actual_crlf_unicode_bytes_at_nonzero_start() -> TestResult {
    for count in [2usize, 3, 64] {
        let text = (0..count)
            .map(|n| format!("Grüße{n}\r\n"))
            .collect::<String>();
        let p = page(&text, true)?;
        let lines = proper_window_lines(&p)?;
        let offers = window_offers(&lines)?;
        assert_eq!(offers, window_offers(&proper_window_lines(&p)?)?);
        let split = count / 2;
        for (choice, expected_first, expected_last, expected_text) in [
            ("W1", 2, 1 + u32::try_from(split)?, lines[..split].iter()),
            (
                "W2",
                2 + u32::try_from(split)?,
                1 + u32::try_from(count)?,
                lines[split..].iter(),
            ),
        ] {
            let raw = format!(r#"{{"window":"{choice}","intent":"Change this bounded part"}}"#);
            let decision = decode_model_reply(&p, &lines, raw.as_bytes())?;
            let HttpNoChangeReplanDecision::Window {
                first,
                last,
                original,
                text_digest,
                ..
            } = decision
            else {
                return Err("offered half was not a smaller window".into());
            };
            assert_eq!((first.get(), last.get()), (expected_first, expected_last));
            assert!(p.range().contains(original.range()));
            assert!(original.range().len() < p.range().len());
            assert!(usize::try_from(last.get() - first.get() + 1)? <= count.div_ceil(2));
            let relative = original.range().start_byte() - p.range().start_byte();
            let end = relative + original.range().len();
            let actual = p
                .text()
                .get(relative as usize..end as usize)
                .ok_or("bad exact range")?;
            let expected_text = expected_text
                .map(|line| &p.text()[line.bytes.clone()])
                .collect::<String>();
            assert_eq!(actual, expected_text);
            assert_eq!(
                text_digest,
                ContentHash::from_bytes(*blake3::hash(actual.as_bytes()).as_bytes())
            );
        }
        let [left, right] = offers;
        assert_eq!(left.first.get(), 2);
        assert_eq!(left.last.get().checked_add(1), Some(right.first.get()));
        assert_eq!(right.last.get(), 1 + u32::try_from(count)?);
    }
    Ok(())
}
#[test]
fn full_window_remains_nonexecutable_core_no_progress_and_single_line_cannot_prepare() -> TestResult
{
    let p = page("lead\nbody\ntail\n", true)?;
    let lines = proper_window_lines(&p)?;
    // Existing semantic guard remains terminal metadata, never a new V6 model offer.
    let d = resolve_window(
        &p,
        &lines,
        2,
        4,
        retained::checked_intent("Same interval".to_owned())?,
    )?;
    assert!(
        matches!(d, HttpNoChangeReplanDecision::NoProgress { first, last, .. } if first.get()==2 && last.get()==4)
    );
    for raw in [
        r#"{"first":2,"last":4,"intent":"Same interval"}"#,
        r#"{"window":"all","intent":"Whole window"}"#,
        r#"{"window":["W1","W2"],"intent":"Union"}"#,
        r#"{"window":"W1+W2","intent":"Union"}"#,
    ] {
        assert_eq!(
            decode_model_reply(&p, &lines, raw.as_bytes()),
            Err(HttpNoChangeReplanFailure::InvalidReply)
        );
    }
    let one = page("one\n", true)?;
    assert!(matches!(
        proper_window_lines(&one),
        Err(HttpNoChangeReplanFailure::NoSmallerWindow)
    ));
    Ok(())
}
#[test]
fn valid_json_secret_is_rejected_without_becoming_a_structural_repair() -> TestResult {
    let p = page("a\nb\n", true)?;
    let lines = proper_window_lines(&p)?;
    for reply in [
        br#"{"first":2,"last":2,"intent":"AKIAIOSFODNN7EXAMPLE"}"#.as_slice(),
        br#"{"first":"bad","last":2,"intent":"AKIAIOSFODNN7EXAMPLE"}"#.as_slice(),
        br#"{"halt":"needEvidence","extra":"AKIAIOSFODNN7EXAMPLE"}"#.as_slice(),
        br#"{"halt":"needEvidence","extra":"\u0041KIAIOSFODNN7EXAMPLE","extra":"safe"}"#.as_slice(),
        br#"{"halt":"needEvidence","extra":{"password":"fixture-secret-value"}}"#.as_slice(),
        br#"{"halt":"needEvidence","extra":[{"nested":"password=fixture-secret-value"}]}"#
            .as_slice(),
        br#"{"halt":"needEvidence","\u0041KIAIOSFODNN7EXAMPLE":false}"#.as_slice(),
    ] {
        assert!(matches!(
            decode_model_reply(&p, &lines, reply),
            Err(HttpNoChangeReplanFailure::UnsafeContent)
        ));
    }
    Ok(())
}
#[test]
fn only_closed_alias_objects_and_safe_bounded_intents_decode() -> TestResult {
    let p = page("a\nb\nc\n", true)?;
    let lines = proper_window_lines(&p)?;
    for reply in [
        r#"{"window":"W1","window":"W2","intent":"Change"}"#,
        r#"{"window":"W1","intent":"Change","version":6}"#,
        r#"{"window":"W3","intent":"Change"}"#,
        r#"{"window":1,"intent":"Change"}"#,
        r#"{"window":"W1","intent":""}"#,
        r#"{"window":"W1","intent":"Change","halt":"tooLarge"}"#,
        r#"["W1","Change"]"#,
        r#"{"halt":"other"}"#,
    ] {
        assert_eq!(
            decode_model_reply(&p, &lines, reply.as_bytes()),
            Err(HttpNoChangeReplanFailure::InvalidReply),
            "accepted invalid V6 shape"
        );
    }
    for (name, expected) in [
        ("needEvidence", HttpNoChangeReplanHalt::NeedEvidence),
        ("tooLarge", HttpNoChangeReplanHalt::TooLarge),
        ("refuse", HttpNoChangeReplanHalt::Refuse),
    ] {
        let raw = format!("{{\"halt\":\"{name}\"}}");
        assert_eq!(
            decode_model_reply(&p, &lines, raw.as_bytes())?,
            HttpNoChangeReplanDecision::Halt(expected)
        );
    }
    for (intent, valid) in [("ä".repeat(48), true), ("ä".repeat(49), false)] {
        let bytes = serde_json::to_vec(&json!({"window":"W1","intent":intent}))?;
        assert_eq!(decode_model_reply(&p, &lines, &bytes).is_ok(), valid);
    }
    Ok(())
}
#[test]
fn offers_reject_over_capacity_or_partial_interior_lines_and_preserve_eof_tail() -> TestResult {
    assert!(matches!(
        proper_window_lines(&page(&"x\n".repeat(65), true)?),
        Err(HttpNoChangeReplanFailure::Capacity)
    ));
    assert!(matches!(
        proper_window_lines(&page(&format!("{}\nx\n", "x".repeat(8192)), true)?),
        Err(HttpNoChangeReplanFailure::Capacity)
    ));
    assert!(matches!(
        proper_window_lines(&page("a\npartial", true)?),
        Err(HttpNoChangeReplanFailure::Capacity)
    ));
    let eof = page("a\nGrüße", false)?;
    let lines = proper_window_lines(&eof)?;
    let d = decode_model_reply(&eof, &lines, br#"{"window":"W2","intent":"Change EOF"}"#)?;
    assert!(
        matches!(d,HttpNoChangeReplanDecision::Window{original,..} if original.range().end_position()==eof.range().end_position())
    );
    Ok(())
}

#[test]
fn retained_window_binds_six_coordinates_and_digest_but_is_not_model_wire() -> TestResult {
    use a3_domain::*;
    let p = page("a\r\nGrüße\r\ntail\r\n", true)?;
    // Explicitly shaped metadata only; this fixture asserts no database authority.
    let anchor = HttpNoChangeReplanAnchor {
        closed_owner: ClosedEditNoChangeOwner {
            worktree: WorktreeId::from_bytes([1; 32]),
            goal: GoalContractReference::initial(TaskId::from_bytes([2; 32])),
            original_goal: GoalOriginalDigest::from_bytes([3; 32]),
            run: AgentRunId::from_bytes([4; 32]),
            ledger: TaskLedgerRevision::new(1)?,
            version: TaskLedgerStoreVersion::new(1)?,
            sequence: RunEventSequence::new(5)?,
            step: TaskStepId::from_bytes([5; 32]),
            attempt: TaskStepAttemptNumber::new(1)?,
            verification: VerificationSpecId::from_bytes([6; 32]),
            index: IndexRunId::from_bytes([7; 32]),
            snapshot: SnapshotId::from_bytes([8; 32]),
            profile: ModelProfileReference::new(
                ModelProfileId::from_bytes([9; 32]),
                ModelProfileVersion::V1,
            ),
        },
        window: ClosedEditWindow::from_recorded(
            EvidenceRef::new(p.revision().clone(), p.range()),
            ContentHash::from_bytes(*blake3::hash(p.text().as_bytes()).as_bytes()),
        )?,
        closure: RunEventId::from_bytes([10; 32]),
        closed_sequence: RunEventSequence::new(6)?,
        closed_version: TaskLedgerStoreVersion::new(2)?,
        parent_job: ContentHash::from_bytes([11; 32]),
        producer_job: ContentHash::from_bytes([12; 32]),
        diagnostic: ContentHash::from_bytes([13; 32]),
        old_job: ContentHash::from_bytes([14; 32]),
        old_source_read: ContentHash::from_bytes([15; 32]),
        old_call_kind: EditPhaseCallKind::Initial,
    };
    let job = ContentHash::from_bytes([16; 32]);
    let lines = proper_window_lines(&p)?;
    let decision = decode_model_reply(
        &p,
        &lines,
        br#"{"window":"W2","intent":"Change bounded part"}"#,
    )?;
    let canonical = decision.canonical_retained_bytes(job)?;
    assert!(canonical.len() <= 512);
    assert_eq!(
        HttpNoChangeReplanDecision::decode_retained(job, &anchor, &canonical)?,
        decision
    );
    assert!(matches!(
        decode_model_reply(&p, &lines, &canonical),
        Err(HttpNoChangeReplanFailure::InvalidReply)
    ));
    let mut changed: serde_json::Value = serde_json::from_slice(&canonical)?;
    changed["range"][3] = json!(1);
    assert!(
        HttpNoChangeReplanDecision::decode_retained(job, &anchor, &serde_json::to_vec(&changed)?)
            .is_err()
    );
    assert!(
        HttpNoChangeReplanDecision::decode_retained(
            ContentHash::from_bytes([17; 32]),
            &anchor,
            &canonical
        )
        .is_err()
    );
    // A valid third proper subset is neither offered nor replayable, even with exact bytes/hash.
    let third = resolve_window(
        &p,
        &lines,
        3,
        3,
        retained::checked_intent("Forged third part".to_owned())?,
    )?;
    let forged = third.canonical_retained_bytes(job)?;
    assert!(HttpNoChangeReplanDecision::decode_retained(job, &anchor, &forged).is_err());
    assert!(encode_checked_decision(&p, &lines, &anchor, job, &third).is_err());
    assert!(encode_checked_decision(&p, &lines, &anchor, job, &decision).is_ok());
    let mut old_version: serde_json::Value = serde_json::from_slice(&canonical)?;
    old_version["version"] = json!(5);
    assert!(
        HttpNoChangeReplanDecision::decode_retained(
            job,
            &anchor,
            &serde_json::to_vec(&old_version)?
        )
        .is_err()
    );
    // Shape-only NoProgress is nonexecutable; it never becomes a V6 source offer.
    let no_progress = resolve_window(
        &p,
        &lines,
        2,
        4,
        retained::checked_intent("No smaller scope".to_owned())?,
    )?;
    let canonical = no_progress.canonical_retained_bytes(job)?;
    assert_eq!(
        HttpNoChangeReplanDecision::decode_retained(job, &anchor, &canonical)?,
        no_progress
    );
    let mut changed: serde_json::Value = serde_json::from_slice(&canonical)?;
    changed["first"] = json!(3);
    assert!(
        HttpNoChangeReplanDecision::decode_retained(job, &anchor, &serde_json::to_vec(&changed)?)
            .is_err()
    );
    Ok(())
}

#[test]
fn deep_or_incomplete_json_cannot_become_a_repairable_v6_structure_error() -> TestResult {
    let p = page("a\nb\n", true)?;
    let lines = proper_window_lines(&p)?;
    let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
    for reply in [
        format!(
            r#"{{"first":2,"last":2,"intent":"Change","early":"\u0041KIAIOSFODNN7EXAMPLE","pad":{deep}}}"#
        ),
        format!(r#"{{"halt":"refuse","x":"\u0041KIAIOSFODNN7EXAMPLE","x":"safe","pad":{deep}}}"#),
        format!(r#"{{"pad":{deep},"late":"\u0041KIAIOSFODNN7EXAMPLE","halt":"refuse"}}"#),
        format!(r#"{{"pad":{deep},"halt":"refuse"}}"#),
        r#"{"halt":"refuse""#.to_owned(),
        "{".to_owned(),
    ] {
        assert!(reply.len() <= MAX_REPLY_BYTES);
        assert_eq!(
            decode_model_reply(&p, &lines, reply.as_bytes()),
            Err(HttpNoChangeReplanFailure::UnsafeInspection)
        );
    }
    assert_eq!(
        decode_model_reply(&p, &lines, &vec![b' '; MAX_REPLY_BYTES + 1]),
        Err(HttpNoChangeReplanFailure::UnsafeInspection)
    );
    // Completely inspected safe JSON with wrong shape keeps the genuine repair path.
    assert_eq!(
        decode_model_reply(&p, &lines, br#"{"halt":"refuse","extra":true}"#),
        Err(HttpNoChangeReplanFailure::InvalidReply)
    );
    assert!(matches!(
        decode_model_reply(&p, &lines, br#"{"window":"W1","intent":"Change"}"#)?,
        HttpNoChangeReplanDecision::Window { .. }
    ));
    Ok(())
}
