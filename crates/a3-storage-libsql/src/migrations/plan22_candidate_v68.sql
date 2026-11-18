-- Fresh68 initialization only. Existing42..67 data is rejected before any write.
-- One new bounded V5 job for one authentic first A-closed HTTP NoChange origin.
-- The old67 receipt, original failure/producers, ledger and event rows remain authoritative.
CREATE TABLE http_no_change_replan_reservations (
    worktree_id BLOB NOT NULL CHECK(length(worktree_id)=32),
    run_id BLOB NOT NULL CHECK(length(run_id)=32),
    closed_job_digest BLOB NOT NULL CHECK(length(closed_job_digest)=32),
    closed_call_kind TEXT NOT NULL CHECK(closed_call_kind IN ('initial','structural_repair')),
    call_kind TEXT NOT NULL CHECK(call_kind IN ('initial','structural_repair')),
    job_digest BLOB NOT NULL CHECK(length(job_digest)=32),
    protocol_version INTEGER NOT NULL DEFAULT 5 CHECK(protocol_version=5),
    anchor_version INTEGER NOT NULL DEFAULT 1 CHECK(anchor_version=1),
    anchor BLOB NOT NULL CHECK(length(anchor) BETWEEN 512 AND 133000),
    closure_sequence INTEGER NOT NULL CHECK(closure_sequence>0),
    closure_event_id BLOB NOT NULL CHECK(length(closure_event_id)=32),
    wire_digest BLOB NOT NULL CHECK(length(wire_digest)=32),
    wire_bytes INTEGER NOT NULL CHECK(wire_bytes BETWEEN 1 AND 16777216),
    prompt_reserve INTEGER NOT NULL CHECK(prompt_reserve BETWEEN 1 AND 4294967295),
    output_reserve INTEGER NOT NULL CHECK(output_reserve BETWEEN 1 AND 4294967295),
    reserved_at INTEGER NOT NULL CHECK(reserved_at>=0),
    deadline INTEGER NOT NULL CHECK(deadline>reserved_at),
    outcome TEXT CHECK(outcome IN ('accepted_reply','no_progress','invalid_structure','rejected_reply','provider_failure','cancelled','unknown','stale','budget_overrun')),
    finished_at INTEGER CHECK(finished_at>=reserved_at),
    reported_prompt INTEGER CHECK(reported_prompt BETWEEN 0 AND 4294967295),
    reported_output INTEGER CHECK(reported_output BETWEEN 0 AND 4294967295),
    -- Only canonical Core records, never model raw bytes or source text.
    proposal BLOB CHECK(length(proposal) BETWEEN 1 AND 512),
    proposal_digest BLOB CHECK(length(proposal_digest)=32),
    PRIMARY KEY(run_id,closed_job_digest,closed_call_kind,call_kind),
    CHECK((outcome IS NULL)=(finished_at IS NULL)),
    CHECK(outcome IS NOT NULL OR (reported_prompt IS NULL AND reported_output IS NULL AND proposal IS NULL AND proposal_digest IS NULL)),
    CHECK((outcome IN ('accepted_reply','no_progress'))=(proposal IS NOT NULL)),
    CHECK((proposal IS NULL)=(proposal_digest IS NULL)),
    CHECK(outcome NOT IN ('accepted_reply','no_progress','invalid_structure') OR (reported_prompt>0 AND reported_output>0 AND reported_prompt IS NOT NULL AND reported_output IS NOT NULL)),
    FOREIGN KEY(worktree_id) REFERENCES worktrees(worktree_id) ON DELETE RESTRICT,
    FOREIGN KEY(run_id,closed_job_digest,closed_call_kind)
        REFERENCES edit_phase_no_change_receipts(run_id,job_digest,call_kind) ON DELETE RESTRICT,
    FOREIGN KEY(run_id,closure_sequence) REFERENCES run_events(run_id,event_sequence) ON DELETE RESTRICT
) STRICT;
CREATE UNIQUE INDEX http_no_change_replan_one_pending_per_run
    ON http_no_change_replan_reservations(run_id) WHERE outcome IS NULL;
CREATE TRIGGER http_no_change_replan_insert_guard BEFORE INSERT ON http_no_change_replan_reservations
WHEN NEW.outcome IS NOT NULL OR NEW.finished_at IS NOT NULL OR NEW.reported_prompt IS NOT NULL
 OR NEW.reported_output IS NOT NULL OR NEW.proposal IS NOT NULL OR NEW.proposal_digest IS NOT NULL
BEGIN SELECT RAISE(ABORT,'HTTP NoChange replan starts unobserved'); END;
CREATE TRIGGER http_no_change_replan_repair_guard BEFORE INSERT ON http_no_change_replan_reservations
WHEN NEW.call_kind='structural_repair' AND NOT EXISTS(
    SELECT 1 FROM http_no_change_replan_reservations p WHERE p.run_id=NEW.run_id
    AND p.closed_job_digest=NEW.closed_job_digest AND p.closed_call_kind=NEW.closed_call_kind
    AND p.call_kind='initial' AND p.outcome='invalid_structure' AND p.job_digest=NEW.job_digest
    AND p.worktree_id=NEW.worktree_id AND p.anchor=NEW.anchor
    AND p.closure_sequence=NEW.closure_sequence AND p.closure_event_id=NEW.closure_event_id
    AND p.deadline=NEW.deadline AND p.finished_at<=NEW.reserved_at
    AND p.wire_digest!=NEW.wire_digest)
BEGIN SELECT RAISE(ABORT,'HTTP NoChange repair requires its own invalid initial'); END;
CREATE TRIGGER http_no_change_replan_update_guard BEFORE UPDATE ON http_no_change_replan_reservations
WHEN OLD.outcome IS NOT NULL OR NEW.outcome IS NULL
 OR NEW.worktree_id!=OLD.worktree_id OR NEW.run_id!=OLD.run_id
 OR NEW.closed_job_digest!=OLD.closed_job_digest OR NEW.closed_call_kind!=OLD.closed_call_kind
 OR NEW.call_kind!=OLD.call_kind OR NEW.job_digest!=OLD.job_digest
 OR NEW.protocol_version!=OLD.protocol_version OR NEW.anchor_version!=OLD.anchor_version OR NEW.anchor!=OLD.anchor
 OR NEW.closure_sequence!=OLD.closure_sequence OR NEW.closure_event_id!=OLD.closure_event_id
 OR NEW.wire_digest!=OLD.wire_digest OR NEW.wire_bytes!=OLD.wire_bytes
 OR NEW.prompt_reserve!=OLD.prompt_reserve OR NEW.output_reserve!=OLD.output_reserve
 OR NEW.reserved_at!=OLD.reserved_at OR NEW.deadline!=OLD.deadline
BEGIN SELECT RAISE(ABORT,'HTTP NoChange replan is immutable'); END;
CREATE TRIGGER http_no_change_replan_delete_guard BEFORE DELETE ON http_no_change_replan_reservations
BEGIN SELECT RAISE(ABORT,'HTTP NoChange replan costs are append-only'); END;
