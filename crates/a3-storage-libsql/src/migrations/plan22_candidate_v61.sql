-- One current-run correction Plan and its one structural repair per observed diagnosis.
-- Accepted proposal data is not a Ledger action or execution authority.
CREATE TABLE http_correction_plan_reservations (
    worktree_id BLOB NOT NULL CHECK(length(worktree_id)=32),
    run_id BLOB NOT NULL CHECK(length(run_id)=32),
    diagnostic_digest BLOB NOT NULL CHECK(length(diagnostic_digest)=32),
    tool_run_id BLOB NOT NULL CHECK(length(tool_run_id)=32),
    tool_attempt INTEGER NOT NULL CHECK(tool_attempt BETWEEN 1 AND 4294967295),
    job_digest BLOB NOT NULL CHECK(length(job_digest)=32),
    call_kind TEXT NOT NULL CHECK(call_kind IN ('initial','structural_repair')),
    anchor BLOB NOT NULL CHECK(length(anchor) BETWEEN 256 AND 133000),
    wire_digest BLOB NOT NULL CHECK(length(wire_digest)=32),
    wire_bytes INTEGER NOT NULL CHECK(wire_bytes BETWEEN 1 AND 16777216),
    prompt_reserve INTEGER NOT NULL CHECK(prompt_reserve BETWEEN 1 AND 4294967295),
    output_reserve INTEGER NOT NULL CHECK(output_reserve BETWEEN 1 AND 4294967295),
    reserved_at INTEGER NOT NULL CHECK(reserved_at>=0),
    deadline INTEGER NOT NULL CHECK(deadline>reserved_at),
    outcome TEXT CHECK(outcome IN ('accepted_reply','invalid_structure','rejected_reply','provider_failure','cancelled','unknown','stale','budget_overrun')),
    finished_at INTEGER CHECK(finished_at>=reserved_at),
    reported_prompt INTEGER CHECK(reported_prompt BETWEEN 0 AND 4294967295),
    reported_output INTEGER CHECK(reported_output BETWEEN 0 AND 4294967295),
    proposal BLOB CHECK(length(proposal) BETWEEN 1 AND 512),
    proposal_digest BLOB CHECK(length(proposal_digest)=32),
    CHECK((outcome IS NULL)=(finished_at IS NULL)),
    CHECK(outcome IS NOT NULL OR (reported_prompt IS NULL AND reported_output IS NULL AND proposal IS NULL AND proposal_digest IS NULL)),
    CHECK((outcome='accepted_reply')=(proposal IS NOT NULL)),
    CHECK((proposal IS NULL)=(proposal_digest IS NULL)),
    CHECK(outcome NOT IN ('accepted_reply','invalid_structure') OR (reported_prompt IS NOT NULL AND reported_output IS NOT NULL AND reported_prompt>0 AND reported_output>0)),
    PRIMARY KEY(run_id,diagnostic_digest,call_kind),
    FOREIGN KEY(run_id) REFERENCES agent_runs(run_id) ON DELETE RESTRICT,
    FOREIGN KEY(tool_run_id,tool_attempt) REFERENCES loopback_mismatch_diagnostics(tool_run_id,attempt_sequence) ON DELETE RESTRICT
) STRICT;
CREATE TRIGGER http_correction_plan_insert_guard BEFORE INSERT ON http_correction_plan_reservations
WHEN NEW.outcome IS NOT NULL OR NEW.finished_at IS NOT NULL OR NEW.reported_prompt IS NOT NULL
 OR NEW.reported_output IS NOT NULL OR NEW.proposal IS NOT NULL OR NEW.proposal_digest IS NOT NULL
BEGIN SELECT RAISE(ABORT,'HTTP correction Plan starts unobserved'); END;
CREATE TRIGGER http_correction_plan_update_guard BEFORE UPDATE ON http_correction_plan_reservations
WHEN OLD.outcome IS NOT NULL OR NEW.outcome IS NULL OR NEW.worktree_id!=OLD.worktree_id OR NEW.run_id!=OLD.run_id
 OR NEW.diagnostic_digest!=OLD.diagnostic_digest OR NEW.tool_run_id!=OLD.tool_run_id OR NEW.tool_attempt!=OLD.tool_attempt
 OR NEW.job_digest!=OLD.job_digest OR NEW.call_kind!=OLD.call_kind OR NEW.anchor!=OLD.anchor
 OR NEW.wire_digest!=OLD.wire_digest OR NEW.wire_bytes!=OLD.wire_bytes
 OR NEW.prompt_reserve!=OLD.prompt_reserve OR NEW.output_reserve!=OLD.output_reserve
 OR NEW.reserved_at!=OLD.reserved_at OR NEW.deadline!=OLD.deadline
BEGIN SELECT RAISE(ABORT,'HTTP correction Plan reservation is immutable'); END;
CREATE TRIGGER http_correction_plan_delete_guard BEFORE DELETE ON http_correction_plan_reservations
BEGIN SELECT RAISE(ABORT,'HTTP correction Plan accounting is append-only'); END;
