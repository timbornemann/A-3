//! Fixed hashes captured through public APIs before extracting the live v3 encoder.
//! Source A: edit-nochange-handoff-004.json SHA-256
//! 15011350c5812b898a40926e99843dc17629139817f522896e61e87349657d13.
//! Capture manifest: closed-nochange-legacy-v3-vectors-001.json SHA-256
//! fd7bdb3883cade1f35ff53fc03fca9f37b49f64273a4d787091dac73674fdd3e.
//! This checks public live outputs against historical constants, not a second encoder.
use super::*;
use a3_domain::{EvidenceRef, PlannedFileAddition};

fn legacy_fixture(source: &str) -> TestResult<Fixture> {
    Fixture::new(ModelPromptSchemaGrounding::FormatFieldOnly, LATE, source)
}

fn check_legacy(
    case: &str,
    phase: PreparedEditPhase,
    page: Option<&AgentSourcePage>,
    expected: &str,
    original_match: Option<bool>,
) {
    // Hex serialization only; the sole job hash producer is PreparedEditPhase.
    let actual = phase
        .job_digest()
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        actual, expected,
        "historical v3 identity changed for {case}"
    );
    let descriptor = phase.no_change_window();
    assert_eq!(
        descriptor.is_some(),
        original_match.is_some(),
        "replace scope changed for {case}"
    );
    let actual_match = descriptor.as_ref().zip(page).map(|(window, original)| {
        window.matches_original(&EvidenceRef::new(
            original.revision().clone(),
            original.range(),
        ))
    });
    assert_eq!(
        actual_match, original_match,
        "Original byte span changed for {case}"
    );
}

#[test]
fn live_edit_job_hashes_match_independent_frozen_a_v3_capture() -> TestResult {
    let lf = legacy_fixture("value = 1\n")?;
    check_legacy(
        "replace_lf",
        PreparedEditPhase::update(lf.state(&lf.run)?, &lf.page)?,
        Some(&lf.page),
        "a2706c83e8c54408ddb8fd183c8cbafc565211374b9e41b85aaf8cf45c7b4d1d",
        Some(true),
    );

    let crlf = legacy_fixture("value = \"Grüße\"\r\n")?;
    check_legacy(
        "replace_crlf_unicode",
        PreparedEditPhase::update(crlf.state(&crlf.run)?, &crlf.page)?,
        Some(&crlf.page),
        "1231d29019bacd5cb0ad517b3a4b4148bda98cfd1207ee41fda2c88afbe74da7",
        Some(true),
    );

    let bom = legacy_fixture("\u{feff}value = \"Grüße\"\r\n")?;
    check_legacy(
        "replace_bom_crlf_unicode",
        PreparedEditPhase::update(bom.state(&bom.run)?, &bom.page)?,
        Some(&bom.page),
        "9dd59f913f892e651836bea771fd604bdcd032fdc959094cefe740edf074ab1f",
        Some(false),
    );
    check_legacy(
        "append_bom_crlf_unicode",
        PreparedEditPhase::append(bom.state(&bom.run)?, &bom.page)?,
        Some(&bom.page),
        "2c6acc6b5a4b919add7002429a240fafc3d9e716773e99b7adae0efbb928a209",
        None,
    );

    let interior = legacy_fixture("head\r\nvalue = \"Grüße\"\r\ntail\r\n")?;
    let start = "head\r\n".len();
    let text = "value = \"Grüße\"\r\n";
    let page = AgentSourcePage::new(
        interior.page.revision().clone(),
        SourceRange::new(
            start,
            start + text.len(),
            SourcePosition::new(1, 0),
            SourcePosition::new(2, 0),
        )?,
        AgentFileStartLine::new(2)?,
        text.to_owned(),
        Some(AgentFileStartLine::new(3)?),
        true,
    )?;
    check_legacy(
        "replace_interior_crlf_unicode_more_lines",
        PreparedEditPhase::update(interior.state(&interior.run)?, &page)?,
        Some(&page),
        "d27f83e7a61139d590d184abce770e47c30cddac9ea1a1a48fef033f5cc97633",
        Some(true),
    );

    let tail = legacy_fixture("value = \"Grüße\"")?;
    check_legacy(
        "append_unterminated_unicode",
        PreparedEditPhase::append(tail.state(&tail.run)?, &tail.page)?,
        Some(&tail.page),
        "d47ac97c0f63b47f92407f7887c3580d23a5fa07f9e0a2d8a814298a7d27ce52",
        None,
    );

    let new_path = RepositoryPath::try_from_bytes(b"server.py".to_vec())?;
    check_legacy(
        "add_nonempty_index",
        PreparedEditPhase::add(lf.state(&lf.run)?, new_path.clone())?,
        None,
        "9a317d508652e2249c5b5c1410b7c5c3cb6ed97af0a9c0d973b49da92c24e2fa",
        None,
    );
    let empty = index_with_files(lf.published.run().snapshot_id(), Vec::new())?;
    let unbound = EditPhaseState::new(&lf.project, &lf.goal, &lf.stored, &lf.run, &empty)?;
    check_legacy(
        "add_empty_index",
        PreparedEditPhase::add(unbound, new_path)?,
        None,
        "70cfc18886e9106db62553ec28bc31c73485a7e303b08c75e33ea1d773904c18",
        None,
    );
    let target = lf.page.revision().path().clone();
    check_legacy(
        "add_empty_index_unbound_subject",
        PreparedEditPhase::add(unbound, target.clone())?,
        None,
        "4b67214427106177e145b1f7c6ee57a9993948e5320f478ff36359bc45d68eb6",
        None,
    );

    let id = TaskStepId::from_bytes([3; 32]);
    let definition = lf
        .stored
        .ledger()
        .step(id)
        .ok_or("step missing")?
        .definition()
        .clone()
        .with_planned_file_addition(PlannedFileAddition::new(target.clone()))?;
    let mut ledger = TaskLedger::new(
        lf.goal.reference(),
        vec![definition],
        TaskLedgerTimestamp::from_unix_millis(1)?,
    )?;
    ledger.start_step(id, lf.run.id(), TaskLedgerTimestamp::from_unix_millis(2)?)?;
    let stored = StoredTaskLedger::new(ledger, lf.stored.version());
    let bound = EditPhaseState::new(&lf.project, &lf.goal, &stored, &lf.run, &empty)?;
    check_legacy(
        "add_empty_index_planned_subject",
        PreparedEditPhase::add(bound, target)?,
        None,
        "e6c744c355668676fbb0b648d7ada006cfc9c761f6f617ac834297ea8cb1c2db",
        None,
    );
    Ok(())
}
