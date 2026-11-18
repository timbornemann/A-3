//! Native deterministic codec regressions; controlled fixtures are not local-model quality trials.
use super::*;

async fn retained_page(f: &CurrentPlanFixture) -> TestResult<AgentSourcePage> {
    let a = f.phase.anchor();
    let first = a
        .source_range
        .start_position()
        .row()
        .checked_add(1)
        .ok_or("row overflow")?;
    let count = a
        .source_range
        .end_position()
        .row()
        .checked_add(u32::from(a.source_range.end_position().column() > 0))
        .ok_or("row overflow")?
        .checked_sub(first)
        .and_then(|count| count.checked_add(1))
        .ok_or("empty original source")?;
    Ok(WorkspaceAgentSourceReader
        .read_page(
            &f.http.fixture.project,
            &a.source,
            &AgentFileInspection::new(
                a.source.path().clone(),
                AgentFileStartLine::new(first)?,
                AgentFileLineCount::new(u16::try_from(count)?)?,
            ),
            &ActiveControl,
        )
        .await?)
}

#[test]
fn http_correction_origin_anchor_existing_bytes_roundtrip_and_reject_malformed_codec() -> TestResult
{
    crate::run_native_libsql_test(async {
        for offered in [false, true] {
            let f = if offered {
                full_fixture(ollama_profile("qwen38-4k:latest")?, false).await?
            } else {
                current_fixture().await?
            };
            let a = f.phase.anchor();
            let bytes = crate::http_correction_plan_repository::anchor_bytes(a);
            assert_eq!(
                crate::http_correction_plan_repository::decode_retained_anchor(
                    &bytes,
                    &f.http.goal
                )?,
                *a
            );
            for end in 0..bytes.len() {
                assert!(
                    crate::http_correction_plan_repository::decode_retained_anchor(
                        &bytes[..end],
                        &f.http.goal
                    )
                    .is_err(),
                    "truncated anchor length {end}"
                );
            }
            let mut extra = bytes.clone();
            extra.push(0);
            assert!(
                crate::http_correction_plan_repository::decode_retained_anchor(
                    &extra,
                    &f.http.goal
                )
                .is_err()
            );
            // Existing v2 offset positions: 13 IDs, 13 little-endian u64s, length, flag, optional descriptor, path.
            for (offset, replacement) in [(0, 3), (525, 2)] {
                let mut bad = bytes.clone();
                bad[offset] = replacement;
                assert!(
                    crate::http_correction_plan_repository::decode_retained_anchor(
                        &bad,
                        &f.http.goal
                    )
                    .is_err()
                );
            }
            for offset in [417, 425, 433, 441, 449, 457, 465] {
                let mut bad = bytes.clone();
                bad[offset..offset + 8].copy_from_slice(&0_u64.to_le_bytes());
                assert!(
                    crate::http_correction_plan_repository::decode_retained_anchor(
                        &bad,
                        &f.http.goal
                    )
                    .is_err(),
                    "zero scalar {offset}"
                );
            }
            for offset in [457, 489, 497, 505, 513] {
                let mut bad = bytes.clone();
                bad[offset..offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
                assert!(
                    crate::http_correction_plan_repository::decode_retained_anchor(
                        &bad,
                        &f.http.goal
                    )
                    .is_err(),
                    "overflow scalar {offset}"
                );
            }
            let mut bad = bytes.clone();
            bad[521..525].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(
                crate::http_correction_plan_repository::decode_retained_anchor(&bad, &f.http.goal)
                    .is_err()
            );
            if offered {
                let mut bad = bytes.clone();
                bad[526] ^= 1;
                assert!(
                    crate::http_correction_plan_repository::decode_retained_anchor(
                        &bad,
                        &f.http.goal
                    )
                    .is_err()
                );
            }
            let mut wrong = a.clone();
            wrong.run = AgentRunId::from_bytes([19; 32]);
            // Codec decoding conveys immutable metadata only; semantic authorization belongs to the origin loader.
            assert_eq!(
                crate::http_correction_plan_repository::decode_retained_anchor(
                    &crate::http_correction_plan_repository::anchor_bytes(&wrong),
                    &f.http.goal
                )?,
                wrong
            );
            let mut wrong_task = bytes.clone();
            wrong_task[33] ^= 1;
            assert!(
                crate::http_correction_plan_repository::decode_retained_anchor(
                    &wrong_task,
                    &f.http.goal
                )
                .is_err()
            );
            drop((f.http.compiler, f.http.refresh, f.http.store));
            f.http.fixture.cleanup()?;
        }
        Ok(())
    })
}

#[test]
fn http_correction_origin_retained_focus_is_canonical_and_same_actual_v4_job() -> TestResult {
    crate::run_native_libsql_test(async {
        let f = current_fixture().await?;
        let page = retained_page(&f).await?;
        let a = f.phase.anchor();
        let failed = f
            .stored
            .ledger()
            .step(a.step)
            .ok_or("failed step missing")?
            .definition();
        let diagnostic = f
            .http
            .store
            .load_loopback_mismatch(&f.http.fixture.project, a.run, a.tool, a.tool_attempt)
            .await?
            .ok_or("diagnostic missing")?;
        let proposal = f.phase.decode_proposal(&reply(&f.phase, "L6")?)?;
        let canonical = f.phase.encode_proposal(&proposal)?;
        assert_eq!(proposal.canonical_retained_bytes()?, canonical);
        assert_eq!(
            HttpCorrectionPlanProposal::decode_retained_focus(a, f.phase.job_digest(), &canonical)?,
            proposal
        );
        assert_eq!(
            HttpCorrectionPlanProposal::verify_retained(
                a,
                &f.http.goal,
                failed,
                &diagnostic,
                &page,
                f.phase.job_digest(),
                &canonical
            )?,
            proposal
        );
        for decision in [
            "L0",
            "L4",
            "L14",
            "L06",
            "L+6",
            "L6extra",
            "S1",
            "needEvidence",
            "tooLarge",
            "refuse",
        ] {
            assert!(
                HttpCorrectionPlanProposal::decode_retained_focus(
                    a,
                    f.phase.job_digest(),
                    &reply(&f.phase, decision)?
                )
                .is_err(),
                "decision {decision}"
            );
        }
        let spaced = format!(" {}", String::from_utf8(canonical.clone())?);
        assert!(
            HttpCorrectionPlanProposal::decode_retained_focus(
                a,
                f.phase.job_digest(),
                spaced.as_bytes()
            )
            .is_err()
        );
        let duplicate =
            String::from_utf8(canonical.clone())?.replacen('{', "{\"decision\":\"L6\",", 1);
        assert!(
            HttpCorrectionPlanProposal::decode_retained_focus(
                a,
                f.phase.job_digest(),
                duplicate.as_bytes()
            )
            .is_err()
        );
        let mut extra: serde_json::Value = serde_json::from_slice(&canonical)?;
        extra["range"] = json!([6, 6]);
        assert!(
            HttpCorrectionPlanProposal::decode_retained_focus(
                a,
                f.phase.job_digest(),
                &serde_json::to_vec(&extra)?
            )
            .is_err()
        );
        let mut bad_intent: serde_json::Value = serde_json::from_slice(&canonical)?;
        bad_intent["intent"] = json!("password=fixture-secret-value");
        assert!(
            HttpCorrectionPlanProposal::decode_retained_focus(
                a,
                f.phase.job_digest(),
                &serde_json::to_vec(&bad_intent)?
            )
            .is_err()
        );
        let mut bad = a.clone();
        bad.source_text = hash(b"different source");
        assert!(
            HttpCorrectionPlanProposal::verify_retained(
                &bad,
                &f.http.goal,
                failed,
                &diagnostic,
                &page,
                f.phase.job_digest(),
                &canonical
            )
            .is_err()
        );
        // Self-consistent current alternate page/hash still cannot reuse the original V4 job or alias.
        let shorter = WorkspaceAgentSourceReader
            .read_page(
                &f.http.fixture.project,
                &a.source,
                &AgentFileInspection::new(
                    a.source.path().clone(),
                    AgentFileStartLine::new(6)?,
                    AgentFileLineCount::new(1)?,
                ),
                &ActiveControl,
            )
            .await?;
        let mut rebound = a.clone();
        rebound.source_range = shorter.range();
        rebound.source_text = hash(shorter.text().as_bytes());
        assert!(
            HttpCorrectionPlanProposal::decode_retained_focus(
                &rebound,
                f.phase.job_digest(),
                &canonical
            )
            .is_ok()
        );
        assert!(
            HttpCorrectionPlanProposal::verify_retained(
                &rebound,
                &f.http.goal,
                failed,
                &diagnostic,
                &shorter,
                f.phase.job_digest(),
                &canonical
            )
            .is_err()
        );
        assert_eq!(
            f.http
                .store
                .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
                .await?
                .calls,
            0
        );
        drop((f.http.compiler, f.http.refresh, f.http.store));
        f.http.fixture.cleanup()
    })
}
