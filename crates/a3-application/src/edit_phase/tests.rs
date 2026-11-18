use super::*;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[test]
fn edit_v2_core_reply_binding_rejects_cross_job_before_semantic_decoding() -> TestResult {
    let owner = ContentHash::from_bytes([11; 32]);
    let other_window = ContentHash::from_bytes([12; 32]);
    for document in [
        br#"{"code":"return 42\n"}"#.as_slice(),
        br#"{"need":"definition"}"#.as_slice(),
        br#"{"tooLarge":true}"#.as_slice(),
        b"invalid JSON".as_slice(),
    ] {
        assert_eq!(
            BoundEditPhaseReply::new(owner, document)?.decode(other_window),
            Err(EditPhaseFailure::WrongReplyBinding)
        );
    }
    assert_eq!(
        BoundEditPhaseReply::new(owner, br#"{"code":"return 42\n"}"#)?.decode(owner),
        Ok(DecodedEditReply::Replace("return 42\n".to_owned()))
    );
    Ok(())
}

#[test]
fn edit_v2_core_reply_binding_bounds_wire_before_allocation_and_hides_payload() -> TestResult {
    let owner = ContentHash::from_bytes([11; 32]);
    let exact = vec![b' '; MAX_EDIT_PHASE_REPLY_BYTES];
    assert!(BoundEditPhaseReply::new(owner, &exact).is_ok());
    let oversized = vec![b' '; MAX_EDIT_PHASE_REPLY_BYTES + 1];
    assert!(matches!(
        BoundEditPhaseReply::new(owner, &oversized),
        Err(EditPhaseFailure::TooLarge)
    ));
    let reply = BoundEditPhaseReply::new(owner, br#"{"code":"private fixture text"}"#)?;
    let debug = format!("{reply:?}");
    assert!(debug.contains("reply_bytes"));
    assert!(!debug.contains("private fixture text"));
    Ok(())
}

fn window(
    bytes: &PatchFileContent,
    range: Range<usize>,
    exact: &str,
    append: bool,
) -> TestResult<ExistingWindow> {
    Ok(ExistingWindow {
        revision: FileRevision::new(
            RepositoryPath::try_from_bytes(b"src/code.py".to_vec())?,
            bytes.content_hash(),
        ),
        range,
        exact: exact.to_owned(),
        append,
    })
}

fn update_bytes(operation: PatchOperation) -> TestResult<Vec<u8>> {
    let PatchOperation::Update(update) = operation else {
        return Err("expected complete PatchUpdate".into());
    };
    Ok(update.content().as_bytes().to_vec())
}

#[test]
fn edit_v2_decoder_accepts_only_one_semantic_payload_and_rejects_legacy_controls() {
    assert_eq!(
        decode_reply(r#"{"code":"café\n"}"#.as_bytes()),
        Ok(DecodedEditReply::Replace("café\n".to_owned()))
    );
    assert_eq!(
        decode_reply(br#"{"code":""}"#),
        Ok(DecodedEditReply::Replace(String::new()))
    );
    for (text, need) in [
        ("definition", EditEvidenceNeed::Definition),
        ("caller", EditEvidenceNeed::Caller),
        ("test", EditEvidenceNeed::Test),
    ] {
        let json = format!("{{\"need\":\"{text}\"}}");
        assert_eq!(
            decode_reply(json.as_bytes()),
            Ok(DecodedEditReply::NeedEvidence(need))
        );
    }
    assert_eq!(
        decode_reply(br#"{"tooLarge":true}"#),
        Ok(DecodedEditReply::TooLarge)
    );
    for invalid in [
        r#"{}"#,
        r#"{"replacement":"x"}"#,
        r#"{"version":2,"code":"x"}"#,
        r#"{"window":"W1","code":"x"}"#,
        r#"{"version":1,"window":"W1","replacement":"x"}"#,
        r#"{"code":"x","code":"y"}"#,
        r#"{"need":"caller","need":"test"}"#,
        r#"{"need":"caller","code":"x"}"#,
        r#"{"tooLarge":false}"#,
        r#"{"tooLarge":true,"code":null}"#,
        r#"{"code":"x","start":0,"end":20}"#,
        r#"{"path":"src/code.py","code":"entire file"}"#,
        r#"["x"]"#,
        r#"[{"code":"x"},{"code":"y"}]"#,
        r#"{"code":"x"} {"code":"y"}"#,
        r#"{"code":null}"#,
        r#"{"tooLarge":true,"tooLarge":true}"#,
    ] {
        assert_eq!(
            decode_reply(invalid.as_bytes()),
            Err(EditPhaseFailure::InvalidReply),
            "{invalid}"
        );
    }
    for invalid in [
        br#"{"code":"x\u0000"}"#.as_slice(),
        br#"{"code":"\uFEFFx"}"#.as_slice(),
    ] {
        assert_eq!(
            decode_reply(invalid),
            Err(EditPhaseFailure::InvalidReplacement)
        );
    }
}

#[test]
fn edit_crlf_canonicalizes_to_lf_and_compiles_original_eol_once() -> TestResult {
    let before = "\u{feff}header = \"Grüße λ🙂\"\r\nvalue = 1\r\n# unchanged\r\n";
    let base = PatchFileContent::try_from_bytes(before.as_bytes().to_vec())?;
    let start = before.find("value = 1").ok_or("target missing")?;
    let end = start + "value = 1\r\n".len();
    let expected = "\u{feff}header = \"Grüße λ🙂\"\r\nvalue = 2\r\n# unchanged\r\n";
    for replacement in ["value = 2\n", "value = 2\r\n"] {
        let json = serde_json::to_vec(&serde_json::json!({
            "code": replacement,
        }))?;
        let DecodedEditReply::Replace(canonical) = decode_reply(&json)? else {
            return Err("replacement was not decoded".into());
        };
        assert_eq!(canonical, "value = 2\n");
        let bytes = update_bytes(compile_existing(
            window(&base, start..end, "value = 1\r\n", false)?,
            &base,
            &canonical,
        )?)?;
        assert_eq!(bytes, expected.as_bytes());
        assert!(!bytes.windows(3).any(|part| part == b"\r\r\n"));
    }
    let mixed = serde_json::to_vec(&serde_json::json!({
        "code": "α\r\nβ\nγ\r\n",
    }))?;
    assert_eq!(
        decode_reply(&mixed),
        Ok(DecodedEditReply::Replace("α\nβ\nγ\n".to_owned()))
    );
    Ok(())
}

#[test]
fn edit_crlf_retains_raw_utf8_limit_before_canonicalization() -> TestResult {
    for stem in [
        "x".repeat(MAX_EDIT_BYTES - 2),
        "λ".repeat((MAX_EDIT_BYTES - 2) / 2),
    ] {
        let exact = format!("{stem}\r\n");
        assert_eq!(exact.len(), MAX_EDIT_BYTES);
        let json = serde_json::to_vec(&serde_json::json!({
            "code": exact,
        }))?;
        let DecodedEditReply::Replace(canonical) = decode_reply(&json)? else {
            return Err("bounded replacement was not decoded".into());
        };
        assert_eq!(canonical.len(), MAX_EDIT_BYTES - 1);
        assert!(!canonical.contains('\r'));
        let over = format!("{stem}x\r\n");
        assert_eq!(over.len(), MAX_EDIT_BYTES + 1);
        let json = serde_json::to_vec(&serde_json::json!({
            "code": over,
        }))?;
        assert_eq!(decode_reply(&json), Err(EditPhaseFailure::TooLarge));
    }
    Ok(())
}

#[test]
fn edit_crlf_still_rejects_bare_cr_bom_and_other_controls() -> TestResult {
    for replacement in [
        "x\ry",
        "x\r\r\n",
        "x\r\n\u{feff}",
        "x\r\n\0",
        "x\r\n\u{000b}",
    ] {
        let json = serde_json::to_vec(&serde_json::json!({
            "code": replacement,
        }))?;
        assert_eq!(
            decode_reply(&json),
            Err(EditPhaseFailure::InvalidReplacement)
        );
    }
    Ok(())
}

#[test]
fn edit_crlf_never_normalizes_source_or_rebinds_base() -> TestResult {
    let json = serde_json::to_vec(&serde_json::json!({
        "code": "value = 2\r\n",
    }))?;
    let DecodedEditReply::Replace(canonical) = decode_reply(&json)? else {
        return Err("replacement was not decoded".into());
    };
    let lf = PatchFileContent::try_from_bytes(b"value = 1\n".to_vec())?;
    let bytes = update_bytes(compile_existing(
        window(&lf, 0..lf.as_bytes().len(), "value = 1\n", false)?,
        &lf,
        &canonical,
    )?)?;
    assert_eq!(bytes, b"value = 2\n");
    let changed = PatchFileContent::try_from_bytes(b"value = 3\n".to_vec())?;
    assert_eq!(
        compile_existing(
            window(&lf, 0..lf.as_bytes().len(), "value = 1\n", false)?,
            &changed,
            &canonical,
        ),
        Err(EditPhaseFailure::InvalidWindow)
    );
    let mixed = PatchFileContent::try_from_bytes(b"value = 1\r\n# rest\n".to_vec())?;
    assert_eq!(
        compile_existing(
            window(
                &mixed,
                0..mixed.as_bytes().len(),
                "value = 1\r\n# rest\n",
                false
            )?,
            &mixed,
            &canonical,
        ),
        Err(EditPhaseFailure::AmbiguousLineEndings)
    );
    Ok(())
}

#[test]
fn evidence_gap_and_capacity_reply_are_closed_regular_outcomes() {
    assert_eq!(
        decode_reply(br#"{"need":"caller"}"#),
        Ok(DecodedEditReply::NeedEvidence(EditEvidenceNeed::Caller))
    );
    assert_eq!(
        decode_reply(br#"{"tooLarge":true}"#),
        Ok(DecodedEditReply::TooLarge)
    );
    assert_eq!(
        decode_reply(br#"{"window":"W2","need":"test"}"#),
        Err(EditPhaseFailure::InvalidReply)
    );
}

#[test]
fn caller_target_allows_partial_declaration_and_only_exterior_layout() -> TestResult {
    use a3_domain::SourcePosition;

    let text = "def other():\n    pass\n\n  def target():\n      return 1\n\n";
    let base = PatchFileContent::try_from_bytes(text.as_bytes().to_vec())?;
    let start = text.find("def target():").ok_or("target declaration")?;
    let end = text.trim_end_matches('\n').len();
    let declaration = SourceRange::new(
        start,
        end,
        SourcePosition::new(3, 2),
        SourcePosition::new(4, 14),
    )?;
    let partial_start = text.find("      return").ok_or("target body")?;
    let partial_range = partial_start..end + 1;
    let partial = window(&base, partial_range.clone(), &text[partial_range], false)?;
    assert!(partial.targets_declaration(&partial.revision, declaration));

    let padded_range = start - 2..text.len();
    let padded = window(&base, padded_range.clone(), &text[padded_range], false)?;
    assert!(padded.targets_declaration(&padded.revision, declaration));
    let wrong_revision = FileRevision::new(
        padded.revision.path().clone(),
        ContentHash::from_bytes([82; 32]),
    );
    assert!(!padded.targets_declaration(&wrong_revision, declaration));

    let foreign = window(&base, 0..start - 2, &text[..start - 2], false)?;
    assert!(!foreign.targets_declaration(&foreign.revision, declaration));
    let crossing = window(&base, 0..text.len(), text, false)?;
    assert!(!crossing.targets_declaration(&crossing.revision, declaration));
    let append = window(&base, start..text.len(), &text[start..], true)?;
    assert!(!append.targets_declaration(&append.revision, declaration));
    Ok(())
}

#[test]
fn edit_decoder_counts_utf8_and_complete_wire_bytes() -> TestResult {
    let make_reply = |count| {
        serde_json::to_vec(&serde_json::json!({
            "code": "é".repeat(count)
        }))
    };
    let exact = make_reply(MAX_EDIT_BYTES / 2)?;
    let DecodedEditReply::Replace(decoded) = decode_reply(&exact)? else {
        return Err("expected a replacement".into());
    };
    assert_eq!(decoded.len(), MAX_EDIT_BYTES);
    let oversized = make_reply(MAX_EDIT_BYTES / 2 + 1)?;
    assert_eq!(decode_reply(&oversized), Err(EditPhaseFailure::TooLarge));
    assert_eq!(
        decode_reply(&vec![b' '; MAX_EDIT_PHASE_REPLY_BYTES + 1]),
        Err(EditPhaseFailure::TooLarge)
    );
    Ok(())
}

#[test]
fn exact_splice_changes_only_second_unicode_match_and_preserves_bom_and_crlf() -> TestResult {
    let before = "\u{feff}let café = 1;\r\nlet café = 1;\r\n// end\r\n";
    let base = PatchFileContent::try_from_bytes(before.as_bytes().to_vec())?;
    let selected = "let café = 1;\r\n";
    let start = before.rfind(selected).ok_or("second line missing")?;
    let edit = window(&base, start..start + selected.len(), selected, false)?;
    assert_eq!(
        update_bytes(compile_existing(edit, &base, "let café = 2;\n")?)?,
        "\u{feff}let café = 1;\r\nlet café = 2;\r\n// end\r\n".as_bytes()
    );
    let first = window(&base, 3..3 + selected.len(), selected, false)?;
    assert_eq!(
        update_bytes(compile_existing(first, &base, "let café = 3;\n")?)?,
        "\u{feff}let café = 3;\r\nlet café = 1;\r\n// end\r\n".as_bytes()
    );
    Ok(())
}

#[test]
fn append_needs_actual_eof_and_keeps_the_non_terminated_tail() -> TestResult {
    let before = "start\r\nlast";
    let base = PatchFileContent::try_from_bytes(before.as_bytes().to_vec())?;
    let tail = window(&base, 7..before.len(), "last", true)?;
    assert_eq!(
        update_bytes(compile_existing(tail, &base, "\nnext\n")?)?,
        b"start\r\nlast\r\nnext\r\n"
    );
    let false_eof = window(&base, 0..7, "start\r\n", true)?;
    assert!(matches!(
        compile_existing(false_eof, &base, "next"),
        Err(EditPhaseFailure::InvalidWindow)
    ));
    Ok(())
}

#[test]
fn stale_hash_malformed_range_split_eol_and_no_change_do_not_make_a_patch() -> TestResult {
    let base = PatchFileContent::try_from_bytes("é\r\nx\n".as_bytes().to_vec())?;
    let modified = PatchFileContent::try_from_bytes("é\r\ny\n".as_bytes().to_vec())?;
    let stable = window(&base, 0..4, "é\r\n", false)?;
    assert!(matches!(
        compile_existing(stable, &modified, "new\n"),
        Err(EditPhaseFailure::InvalidWindow)
    ));
    for malformed in [
        window(&base, 1..4, "é\n", false)?,
        window(&base, 3..4, "\n", false)?,
        window(&base, 0..4, "éx\n", false)?,
    ] {
        assert!(matches!(
            compile_existing(malformed, &base, "new\n"),
            Err(EditPhaseFailure::InvalidWindow)
        ));
    }
    let mixed = window(&base, 0..6, "é\r\nx\n", false)?;
    assert!(matches!(
        compile_existing(mixed, &base, "new\n"),
        Err(EditPhaseFailure::AmbiguousLineEndings)
    ));
    let unchanged = window(&base, 0..4, "é\r\n", false)?;
    assert!(matches!(
        compile_existing(unchanged, &base, "é\n"),
        Err(EditPhaseFailure::NoChange)
    ));
    Ok(())
}

#[test]
fn no_change_is_full_file_identity_after_actual_eol_and_boundary_lowering() -> TestResult {
    for eol in ["\n", "\r\n"] {
        let prefix = format!("\u{feff}# retained Unicode λ{eol}");
        let exact = format!("    value = 1{eol}");
        let suffix = format!("    next_value = 2{eol}");
        let full = format!("{prefix}{exact}{suffix}");
        let base = PatchFileContent::try_from_bytes(full.as_bytes().to_vec())?;
        let start = prefix.len();
        let end = start + exact.len();
        // The model can omit this complete-line separator. Core restores it;
        // NoChange is decided from the lowered full bytes, not raw JSON equality.
        assert!(matches!(
            compile_existing(
                window(&base, start..end, &exact, false)?,
                &base,
                "    value = 1"
            ),
            Err(EditPhaseFailure::NoChange)
        ));
        let changed = compile_existing(
            window(&base, start..end, &exact, false)?,
            &base,
            "    value = 3",
        )?;
        assert_eq!(
            update_bytes(changed)?,
            format!("{prefix}    value = 3{eol}{suffix}").as_bytes()
        );
    }
    Ok(())
}

#[test]
fn interior_complete_line_restores_only_its_final_separator_in_original_eol() -> TestResult {
    for eol in ["\n", "\r\n"] {
        let prefix = format!("\u{feff}# Grüße λ🙂{eol}");
        let exact = format!("        body = b'old'{eol}");
        let suffix = format!("        self.send_response(200){eol}");
        let before = format!("{prefix}{exact}{suffix}");
        let base = PatchFileContent::try_from_bytes(before.as_bytes().to_vec())?;
        let raw = "        body = b'new'\n        self.wfile.write(body)";
        for code in [
            raw.to_owned(),
            raw.replace('\n', "\r\n"),
            format!("{raw}\n"),
        ] {
            let document = serde_json::to_vec(&serde_json::json!({"code":code}))?;
            let DecodedEditReply::Replace(canonical) = decode_reply(&document)? else {
                return Err("semantic code missing".into());
            };
            let next = update_bytes(compile_existing(
                window(
                    &base,
                    prefix.len()..prefix.len() + exact.len(),
                    &exact,
                    false,
                )?,
                &base,
                &canonical,
            )?)?;
            let expected = format!("{prefix}{}{eol}{suffix}", raw.replace('\n', eol));
            assert_eq!(next, expected.as_bytes());
            assert!(next.starts_with(prefix.as_bytes()) && next.ends_with(suffix.as_bytes()));
            assert!(!next.windows(3).any(|part| part == b"\r\r\n"));
            // This proves the byte boundary only, never syntax or functional correctness.
            assert!(!String::from_utf8(next)?.contains("write(body)        self.send_response"));
        }
    }
    let before = "\u{feff}old\nnext\n";
    let base = PatchFileContent::try_from_bytes(before.as_bytes().to_vec())?;
    assert_eq!(
        update_bytes(compile_existing(
            window(&base, 3..7, "old\n", false)?,
            &base,
            "new"
        )?)?,
        "\u{feff}new\nnext\n".as_bytes()
    );
    Ok(())
}

#[test]
fn interior_separator_does_not_reinterpret_delete_eof_partial_or_append() -> TestResult {
    for (before, start, end, exact, append, code, expected) in [
        ("old\nnext\n", 0, 4, "old\n", false, "", "next\n"),
        ("old\n", 0, 4, "old\n", false, "new", "new"),
        (
            "prefix old\nnext\n",
            7,
            11,
            "old\n",
            false,
            "new",
            "prefix newnext\n",
        ),
        (
            "old suffix\nnext\n",
            0,
            3,
            "old",
            false,
            "new",
            "new suffix\nnext\n",
        ),
        ("old\n", 0, 4, "old\n", true, "new", "old\nnew"),
    ] {
        let base = PatchFileContent::try_from_bytes(before.as_bytes().to_vec())?;
        assert_eq!(
            update_bytes(compile_existing(
                window(&base, start..end, exact, append)?,
                &base,
                code
            )?)?,
            expected.as_bytes()
        );
    }
    Ok(())
}

#[test]
fn interior_separator_is_core_owned_after_raw_code_limit_and_live_base_validation() -> TestResult {
    let base = PatchFileContent::try_from_bytes(b"old\nnext\n".to_vec())?;
    let code = "x".repeat(MAX_EDIT_BYTES);
    let document = serde_json::to_vec(&serde_json::json!({"code":code}))?;
    let DecodedEditReply::Replace(canonical) = decode_reply(&document)? else {
        return Err("semantic code missing".into());
    };
    assert_eq!(canonical.len(), MAX_EDIT_BYTES);
    let next = update_bytes(compile_existing(
        window(&base, 0..4, "old\n", false)?,
        &base,
        &canonical,
    )?)?;
    assert_eq!(next.len(), MAX_EDIT_BYTES + "\nnext\n".len());
    assert_eq!(&next[MAX_EDIT_BYTES..], b"\nnext\n");
    let document = serde_json::to_vec(&serde_json::json!({"code":"x".repeat(MAX_EDIT_BYTES+1)}))?;
    assert_eq!(decode_reply(&document), Err(EditPhaseFailure::TooLarge));
    let changed = PatchFileContent::try_from_bytes(b"old\nchanged\n".to_vec())?;
    assert_eq!(
        compile_existing(window(&base, 0..4, "old\n", false)?, &changed, "new"),
        Err(EditPhaseFailure::InvalidWindow)
    );
    Ok(())
}

#[test]
fn edit_v3_unknown_need_is_closed_semantic_request_without_raw_text() {
    assert_eq!(
        decode_reply(br#"{"need":"some free command"}"#),
        Ok(DecodedEditReply::NeedEvidence(EditEvidenceNeed::Unknown))
    );
}
