//! Native ADR-0124 contracts using the real acquired source, parser, executor and store.
use super::*;

async fn current_index(f: &HttpEditFixture) -> TestResult<PublishedIndex> {
    f.http
        .store
        .latest_published_index(&f.http.fixture.project, &ActiveControl)
        .await?
        .ok_or_else(|| "current index missing".into())
}

fn wire(request: &CompiledEditPhaseRequest) -> TestResult<EditPhaseWireCost> {
    let adapter = OllamaModelProvider::for_projected_wire(
        OllamaEndpoint::parse("http://127.0.0.1:11434")?,
        Arc::new(LocalOnlyOllamaEndpointPolicy),
    )?;
    let projection = adapter.project_wire_request(request.request())?;
    let budget = PhaseWireBudget::fit(projection, request.request().profile())?;
    Ok(EditPhaseWireCost::new(
        projection.digest(),
        projection.request_bytes(),
        budget.prompt_tokens(),
        budget.output_tokens(),
    )?)
}

async fn execute_reply(
    f: &HttpEditFixture,
    phase: PreparedEditPhase,
    request: &CompiledEditPhaseRequest,
    raw: String,
) -> TestResult<Result<EditPhaseExecutionReport, EditPhaseExecutionFailure>> {
    let current = current_index(f).await?;
    let provider = NativeEdit {
        encoder: OllamaModelProvider::for_projected_wire(
            OllamaEndpoint::parse("http://127.0.0.1:11434")?,
            Arc::new(LocalOnlyOllamaEndpointPolicy),
        )?,
        store: &f.http.store,
        project: &f.http.fixture.project,
        run: f.run.id(),
        reply: raw,
        calls: AtomicUsize::new(0),
    };
    let patch_tool = WorkspacePatchAdapter::with_change_window_preview();
    let result = Box::pin(
        ExecuteEditPhase::new(&provider, &*f.http.store, &patch_tool).execute(
            phase,
            state(f, &current)?,
            request,
            after_phases(f).await?,
            ModelRequestTimeout::DEFAULT,
            &ActiveControl,
        ),
    )
    .await;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    Ok(result)
}

async fn scalar(f: &HttpEditFixture, sql: &str) -> TestResult<i64> {
    let db = f.http.fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    let mut rows = connection.query(sql, ()).await?;
    let row = rows.next().await?.ok_or("expected one aggregate row")?;
    let value = row.get(0)?;
    assert!(rows.next().await?.is_none());
    Ok(value)
}

async fn charged(f: &HttpEditFixture) -> TestResult<EditPhaseReservedUsage> {
    Ok(f.http
        .store
        .load_edit_phase_usage(&f.http.fixture.project, f.run.id())
        .await?)
}

async fn assert_settled(
    f: &HttpEditFixture,
    request: &CompiledEditPhaseRequest,
    expected_outcome: &str,
    expected_halt: Option<&str>,
    before: EditPhaseReservedUsage,
) -> TestResult<()> {
    let db = f.http.fixture.candidate_database().await?;
    let connection = db.connection_for_operation().await?;
    let mut rows = connection
        .query(
            "SELECT j.request_digest, j.halt_kind, e.outcome, e.reported_prompt, \
             e.reported_output, e.terminal_reply_kind \
             FROM edit_phase_jobs j JOIN edit_phase_reservations e USING(run_id,job_digest,call_kind) \
             WHERE j.run_id=?1 AND j.job_digest=?2 AND j.call_kind='initial'",
            libsql::params![f.run.id().as_bytes().to_vec(), request.job_digest().as_bytes().to_vec()],
        )
        .await?;
    let row = rows.next().await?.ok_or("owned edit settlement missing")?;
    assert_eq!(
        row.get::<Vec<u8>>(0)?.as_slice(),
        request.request_digest().as_bytes()
    );
    assert_eq!(row.get::<Option<String>>(1)?.as_deref(), expected_halt);
    assert_eq!(row.get::<String>(2)?, expected_outcome);
    assert_eq!(row.get::<i64>(3)?, 128);
    assert_eq!(row.get::<i64>(4)?, 32);
    assert!(row.get::<Option<String>>(5)?.is_none());
    assert!(rows.next().await?.is_none());
    drop((row, rows, connection, db));
    let cost = wire(request)?;
    assert_eq!(
        charged(f).await?,
        EditPhaseReservedUsage {
            calls: before.calls + 1,
            prompt_tokens: before.prompt_tokens + u64::from(cost.prompt_estimate()),
            output_tokens: before.output_tokens + u64::from(cost.output_reserve()),
            ..before
        }
    );
    Ok(())
}

fn cleanup(f: HttpEditFixture) -> TestResult {
    drop((f.http.store, f.http.refresh, f.http.compiler));
    f.http.fixture.cleanup()
}

#[test]
fn edit_scope_frame_rejects_real_parent_escape_before_patch_and_stays_paid_on_reopen() -> TestResult
{
    crate::run_native_libsql_test(async {
        let (f, phase, request, page) = no_change::boundaries::prepared_16k().await?;
        assert_eq!(
            (page.range().start_byte(), page.range().end_byte()),
            (719, 1089)
        );
        assert_eq!(page.text().len(), 370);
        assert!(request.scope_frame_digest().is_some());
        let before = charged(&f).await?;
        let mutations = scalar(&f, "SELECT count(*) FROM mutation_attempts").await?;
        assert_eq!(
            request.scope_frame_index(),
            Some(current_index(&f).await?.run().id())
        );
        let schema = request
            .request()
            .structured_output()
            .ok_or("Edit schema missing")?;
        assert!(!schema.value().to_string().contains("\"need\""));
        let original = fs::read(f.http.root.join("server.py"))?;
        assert_eq!(original, FROZEN_SOURCE.as_bytes());
        assert_eq!(ORIGINAL_QWEN_REPLACEMENT.len(), 1670);

        // Reproduce the model's exact formerly accepted full-file bytes without publishing them.
        let mut escaped = original[..719].to_vec();
        escaped.extend_from_slice(ORIGINAL_QWEN_REPLACEMENT.as_bytes());
        escaped.extend_from_slice(&original[1089..]);
        assert_eq!(escaped.len(), 3043);
        assert_eq!(
            blake3::hash(&escaped).to_hex().as_str(),
            "6ac7f0b8441be27f287f8328171f3705467a2be944db298a6b5896a63ea5c1f5",
        );

        let decisions = scalar(&f, "SELECT count(*) FROM policy_decisions").await?;
        let read = f
            .http
            .store
            .load_source_read_state(&f.http.fixture.project, phase.anchor())
            .await?;
        assert!(read.acquired().is_some());
        let pending = read
            .progress()
            .pending()
            .cloned()
            .ok_or("pending original")?;
        let deliveries = read.progress().receipts().to_vec();
        let result = execute_reply(
            &f,
            phase,
            &request,
            json!({"code": ORIGINAL_QWEN_REPLACEMENT}).to_string(),
        )
        .await?;
        assert!(
            matches!(
                result,
                Err(EditPhaseExecutionFailure::Reply(
                    EditPhaseFailure::ScopeFrameMismatch
                ))
            ),
            "actual result: {result:?}"
        );
        let f = reopen(f).await?;
        assert_settled(&f, &request, "rejected_reply", None, before).await?;
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM policy_decisions").await?,
            decisions
        );
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM mutation_attempts").await?,
            mutations
        );
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
            0
        );
        assert!(
            f.http
                .store
                .load_edit_halt(&f.http.fixture.project, &request)
                .await?
                .is_none()
        );
        let current = current_index(&f).await?;
        let read = f
            .http
            .store
            .load_source_read_state(&f.http.fixture.project, state(&f, &current)?.anchor())
            .await?;
        // Reservation consumes its acquire slot. The unresolved read earns no delivery.
        assert!(read.acquired().is_none());
        assert_eq!(read.progress().pending(), Some(&pending));
        assert_eq!(read.progress().receipts(), deliveries.as_slice());
        assert_eq!(fs::read(f.http.root.join("server.py"))?, original);
        cleanup(f)
    })
}

#[test]
fn edit_scope_frame_allows_body_rename_removal_and_nested_declaration() -> TestResult {
    crate::run_native_libsql_test(async {
        for kind in ["body", "rename", "remove", "nested"] {
            let (f, phase, request, page) = no_change::boundaries::prepared_16k().await?;
            assert!(request.scope_frame_digest().is_some());
            let before = charged(&f).await?;
            let mutations = scalar(&f, "SELECT count(*) FROM mutation_attempts").await?;
            let replacement = match kind {
                "body" => page
                    .text()
                    .replacen(r#"b"Hello, world!""#, r#"b"Hello, world!\n""#, 1),
                "rename" => page.text().replacen("def do_GET", "def do_LOCAL", 1),
                "remove" => String::new(),
                "nested" => page.text().replacen(
                    "    def do_GET(self):\n",
                    "    def do_GET(self):\n        def body():\n            return b'response'\n",
                    1,
                ),
                _ => return Err("invalid fixture variant".into()),
            };
            assert_ne!(replacement, page.text());
            let report =
                execute_reply(&f, phase, &request, json!({"code":replacement}).to_string())
                    .await??;
            assert!(
                matches!(report.outcome, EditPhaseOutcome::Proposal(_)),
                "{kind}"
            );
            assert_eq!(
                report
                    .source_delivery
                    .ok_or("actual target delivery")?
                    .source()
                    .range(),
                page.range()
            );
            let f = reopen(f).await?;
            assert_settled(&f, &request, "accepted_reply", None, before).await?;
            assert_eq!(
                scalar(&f, "SELECT count(*) FROM mutation_attempts").await?,
                mutations
            );
            assert_eq!(
                fs::read(f.http.root.join("server.py"))?,
                FROZEN_SOURCE.as_bytes()
            );
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn edit_scope_frame_need_and_size_keep_framed_descriptive_receipts_on_reopen() -> TestResult {
    crate::run_native_libsql_test(async {
        for (raw, halt, tag) in [
            (
                r#"{"need":"test"}"#,
                EditPhaseHalt::NeedEvidence(EditEvidenceNeed::Test),
                "test",
            ),
            (r#"{"tooLarge":true}"#, EditPhaseHalt::TooLarge, "too_large"),
        ] {
            let (f, phase, request, _) = no_change::boundaries::prepared_16k().await?;
            assert!(request.scope_frame_digest().is_some());
            let before = charged(&f).await?;
            let mutations = scalar(&f, "SELECT count(*) FROM mutation_attempts").await?;
            let report = execute_reply(&f, phase, &request, raw.to_owned()).await??;
            assert!(report.source_delivery.is_none());
            let observed = match report.outcome {
                EditPhaseOutcome::NeedEvidence(need) => EditPhaseHalt::NeedEvidence(need),
                EditPhaseOutcome::TooLarge => EditPhaseHalt::TooLarge,
                _ => return Err("non-code answer yielded a proposal".into()),
            };
            assert_eq!(observed, halt);
            let f = reopen(f).await?;
            let receipt = f
                .http
                .store
                .load_edit_halt(&f.http.fixture.project, &request)
                .await?
                .ok_or("framed descriptive halt receipt missing")?;
            assert_eq!(receipt.request_digest(), request.request_digest());
            assert_eq!(receipt.disposition(), halt);
            assert_settled(&f, &request, "accepted_reply", Some(tag), before).await?;
            assert_eq!(
                scalar(&f, "SELECT count(*) FROM mutation_attempts").await?,
                mutations
            );
            assert_eq!(
                scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
                0
            );
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn edit_scope_frame_public_store_futures_count_preopen_delay_and_preserve_debits() -> TestResult {
    crate::run_native_libsql_test(async {
        let (f, phase, request, _) = no_change::boundaries::prepared_16k().await?;
        assert!(request.scope_frame_digest().is_some());
        let before = charged(&f).await?;
        let project = &f.http.fixture.project;
        let deadline = f.run.created_at().unix_millis() + f.run.budget().duration_limit().millis();
        let near_deadline = at(deadline - 1000)?;
        let cost = wire(&request)?;
        let delay = std::time::Duration::from_millis(1200);

        // Delay the first poll on the owned test thread, before any store I/O.
        let attempted =
            f.http
                .store
                .reserve_edit_job(project, &phase, &request, cost, near_deadline);
        std::thread::sleep(delay);
        assert!(matches!(
            attempted.await,
            Err(EditPhaseReservationFailure::DeadlineReached)
        ));
        assert_eq!(charged(&f).await?, before);

        // The same request still admits before the deadline with no additional elapsed delay.
        let permit = f
            .http
            .store
            .reserve_edit_job(project, &phase, &request, cost, after_phases(&f).await?)
            .await?;
        assert_eq!(permit.request_digest(), Some(request.request_digest()));
        let pending = f.http.store.finish_edit_job(
            project,
            permit,
            &request,
            EditPhaseObservation::new(
                EditPhaseObservationOutcome::AcceptedReply,
                Some(128),
                Some(32),
                near_deadline,
            ),
            Some(EditPhaseHalt::TooLarge),
        );
        std::thread::sleep(delay);
        assert!(matches!(
            pending.await,
            Err(EditPhaseReservationFailure::DeadlineReached)
        ));
        let f = reopen(f).await?;
        assert_settled(&f, &request, "stale", None, before).await?;
        assert!(
            f.http
                .store
                .load_edit_halt(&f.http.fixture.project, &request)
                .await?
                .is_none()
        );
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
            0
        );
        cleanup(f)
    })
}

#[test]
fn edit_scope_frame_settlement_cannot_hide_reported_prompt_overrun() -> TestResult {
    crate::run_native_libsql_test(async {
        for outcome in [
            EditPhaseObservationOutcome::RejectedReply,
            EditPhaseObservationOutcome::InvalidStructure,
            EditPhaseObservationOutcome::AcceptedReply,
            EditPhaseObservationOutcome::DeadlineReached,
        ] {
            let (f, phase, request, _) = no_change::boundaries::prepared_16k().await?;
            assert!(request.scope_frame_digest().is_some());
            let before = charged(&f).await?;
            let project = &f.http.fixture.project;
            let cost = wire(&request)?;
            let permit = f
                .http
                .store
                .reserve_edit_job(project, &phase, &request, cost, after_phases(&f).await?)
                .await?;
            let observed_at = if matches!(
                outcome,
                EditPhaseObservationOutcome::RejectedReply
                    | EditPhaseObservationOutcome::InvalidStructure
            ) {
                at(permit.reserved_at().unix_millis() + 1)?
            } else {
                permit.deadline()
            };
            let reported_prompt = cost.prompt_estimate() + 1;
            assert!(matches!(
                f.http
                    .store
                    .finish_edit_job(
                        project,
                        permit,
                        &request,
                        EditPhaseObservation::new(
                            outcome,
                            Some(reported_prompt),
                            Some(64),
                            observed_at,
                        ),
                        None,
                    )
                    .await,
                Err(EditPhaseReservationFailure::BudgetExhausted(
                    AgentBudgetDimension::PromptTokens
                ))
            ));
            let f = reopen(f).await?;
            assert_eq!(
                charged(&f).await?,
                EditPhaseReservedUsage {
                    calls: before.calls + 1,
                    prompt_tokens: before.prompt_tokens + u64::from(reported_prompt),
                    output_tokens: before.output_tokens + u64::from(cost.output_reserve()),
                    ..before
                }
            );
            assert_eq!(
            scalar(&f, "SELECT count(*) FROM edit_phase_reservations WHERE outcome='budget_overrun' AND reported_output=64 AND terminal_reply_kind IS NULL").await?,
            1
        );
            assert_eq!(
                scalar(
                    &f,
                    "SELECT count(*) FROM edit_phase_jobs WHERE halt_kind IS NOT NULL"
                )
                .await?,
                0
            );
            assert_eq!(
                scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
                0
            );
            cleanup(f)?;
        }
        Ok(())
    })
}

#[test]
fn edit_scope_frame_deadline_after_terminal_writes_rolls_back_halt_and_keeps_debit() -> TestResult {
    crate::run_native_libsql_test(async {
        let (f, phase, request, _) = no_change::boundaries::prepared_16k().await?;
        assert!(request.scope_frame_digest().is_some());
        let before = charged(&f).await?;
        let project = &f.http.fixture.project;
        let permit = f
            .http
            .store
            .reserve_edit_job(
                project,
                &phase,
                &request,
                wire(&request)?,
                after_phases(&f).await?,
            )
            .await?;
        let at_before_deadline = at(permit
            .deadline()
            .unix_millis()
            .checked_sub(2000)
            .ok_or("deadline")?)?;
        let run = f.run.id();
        let job = request.job_digest();
        let result = crate::edit_phase_reservation_repository::finish_edit_job_after_writes_for_test(
            &f.http.store,
            project,
            permit,
            &request,
            EditPhaseObservation::new(
                EditPhaseObservationOutcome::AcceptedReply,
                Some(128),
                Some(32),
                at_before_deadline,
            ),
            Some(EditPhaseHalt::TooLarge),
            move |tx| {
                let connection = std::ops::Deref::deref(tx).clone();
                async move {
                    let mut rows = connection
                        .query(
                            "SELECT j.halt_kind, e.outcome, e.reported_prompt, e.reported_output \
                             FROM edit_phase_jobs j JOIN edit_phase_reservations e USING(run_id,job_digest,call_kind) \
                             WHERE j.run_id=?1 AND j.job_digest=?2 AND j.call_kind='initial'",
                            libsql::params![run.as_bytes().to_vec(), job.as_bytes().to_vec()],
                        )
                        .await
                        .map_err(|_| EditPhaseReservationFailure::Unavailable)?;
                    let row = rows
                        .next()
                        .await
                        .map_err(|_| EditPhaseReservationFailure::Unavailable)?
                        .ok_or(EditPhaseReservationFailure::InvalidStoredData)?;
                    let content_error = |_| EditPhaseReservationFailure::InvalidStoredData;
                    assert_eq!(row.get::<String>(0).map_err(content_error)?, "too_large");
                    assert_eq!(row.get::<String>(1).map_err(content_error)?, "accepted_reply");
                    assert_eq!(row.get::<i64>(2).map_err(content_error)?, 128);
                    assert_eq!(row.get::<i64>(3).map_err(content_error)?, 32);
                    assert!(rows.next().await.map_err(|_| EditPhaseReservationFailure::Unavailable)?.is_none());
                    drop((row, rows, connection));
                    // This owned delay occurs only after the actual transactional writes.
                    std::thread::sleep(std::time::Duration::from_millis(2200));
                    Ok(())
                }
            },
        )
        .await;
        assert_eq!(result, Err(EditPhaseReservationFailure::DeadlineReached));
        let f = reopen(f).await?;
        assert_settled(&f, &request, "stale", None, before).await?;
        assert!(
            f.http
                .store
                .load_edit_halt(&f.http.fixture.project, &request)
                .await?
                .is_none()
        );
        assert_eq!(
            scalar(&f, "SELECT count(*) FROM edit_phase_no_change_receipts").await?,
            0
        );
        cleanup(f)
    })
}

// Inert exact 1,670-byte reply payload from the independently reviewed Qwen capture.
const ORIGINAL_QWEN_REPLACEMENT: &str = r##"import argparse
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/":
            body = b"Hello, world!\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self._not_found()

    def do_POST(self):
        self._method_not_allowed()

    def do_PUT(self):
        self._method_not_allowed()

    def do_DELETE(self):
        self._method_not_allowed()

    def do_PATCH(self):
        self._method_not_allowed()

    def _not_found(self):
        self.send_response(404)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def _method_not_allowed(self):
        self.send_response(405)
        self.send_header("Allow", "GET")
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", "0")
        self.end_headers()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()

    try:
        server = HTTPServer((args.host, args.port), Handler)
    except OSError:
        sys.exit(1)

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
"##;
