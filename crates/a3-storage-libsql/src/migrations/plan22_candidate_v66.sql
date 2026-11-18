-- Fresh66 initialization only. Existing42..65 data is rejected before any write.
-- One non-executable context-intent job and its one structural repair per diagnosis.
-- The exact accepted V4 parent and actual source acquisition remain independent authorities.
CREATE TABLE http_context_intent_reservations (
    worktree_id BLOB NOT NULL CHECK(length(worktree_id)=32),
    run_id BLOB NOT NULL CHECK(length(run_id)=32),
    diagnostic_digest BLOB NOT NULL CHECK(length(diagnostic_digest)=32),
    call_kind TEXT NOT NULL CHECK(call_kind IN ('initial','structural_repair')),
    parent_call_kind TEXT NOT NULL CHECK(parent_call_kind IN ('initial','structural_repair')),
    parent_job_digest BLOB NOT NULL CHECK(length(parent_job_digest)=32),
    parent_proposal_digest BLOB NOT NULL CHECK(length(parent_proposal_digest)=32),
    source_read_id BLOB NOT NULL CHECK(length(source_read_id)=32),
    job_digest BLOB NOT NULL CHECK(length(job_digest)=32),
    protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version=1),
    anchor BLOB NOT NULL CHECK(length(anchor) BETWEEN 256 AND 137096),
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
    -- Canonical {intent:null} is also an accepted reply: a regular non-mutating halt.
    reply BLOB CHECK(length(reply) BETWEEN 1 AND 1024),
    reply_digest BLOB CHECK(length(reply_digest)=32),
    CHECK((outcome IS NULL)=(finished_at IS NULL)),
    CHECK(outcome IS NOT NULL OR (reported_prompt IS NULL AND reported_output IS NULL AND reply IS NULL AND reply_digest IS NULL)),
    CHECK((outcome='accepted_reply')=(reply IS NOT NULL)),
    CHECK((reply IS NULL)=(reply_digest IS NULL)),
    CHECK(outcome NOT IN ('accepted_reply','invalid_structure') OR (reported_prompt IS NOT NULL AND reported_output IS NOT NULL AND reported_prompt>0 AND reported_output>0)),
    PRIMARY KEY(run_id,diagnostic_digest,call_kind),
    FOREIGN KEY(run_id,diagnostic_digest,parent_call_kind)
        REFERENCES http_correction_plan_reservations(run_id,diagnostic_digest,call_kind) ON DELETE RESTRICT,
    FOREIGN KEY(source_read_id) REFERENCES source_read_acquisitions(source_read_id) ON DELETE RESTRICT
) STRICT;
-- One actual acquisition can settle as accepted once, including a canonical null halt.
CREATE UNIQUE INDEX http_context_intent_one_accepted_per_source_read
    ON http_context_intent_reservations(source_read_id) WHERE outcome='accepted_reply';
CREATE TRIGGER http_context_intent_insert_guard BEFORE INSERT ON http_context_intent_reservations
WHEN NEW.outcome IS NOT NULL OR NEW.finished_at IS NOT NULL OR NEW.reported_prompt IS NOT NULL
 OR NEW.reported_output IS NOT NULL OR NEW.reply IS NOT NULL OR NEW.reply_digest IS NOT NULL
BEGIN SELECT RAISE(ABORT,'HTTP context intent starts unobserved'); END;
CREATE TRIGGER http_context_intent_update_guard BEFORE UPDATE ON http_context_intent_reservations
WHEN OLD.outcome IS NOT NULL OR NEW.outcome IS NULL OR NEW.worktree_id!=OLD.worktree_id OR NEW.run_id!=OLD.run_id
 OR NEW.diagnostic_digest!=OLD.diagnostic_digest OR NEW.call_kind!=OLD.call_kind
 OR NEW.parent_call_kind!=OLD.parent_call_kind OR NEW.parent_job_digest!=OLD.parent_job_digest
 OR NEW.parent_proposal_digest!=OLD.parent_proposal_digest OR NEW.source_read_id!=OLD.source_read_id
 OR NEW.job_digest!=OLD.job_digest OR NEW.protocol_version!=OLD.protocol_version OR NEW.anchor!=OLD.anchor
 OR NEW.wire_digest!=OLD.wire_digest OR NEW.wire_bytes!=OLD.wire_bytes
 OR NEW.prompt_reserve!=OLD.prompt_reserve OR NEW.output_reserve!=OLD.output_reserve
 OR NEW.reserved_at!=OLD.reserved_at OR NEW.deadline!=OLD.deadline
BEGIN SELECT RAISE(ABORT,'HTTP context intent reservation is immutable'); END;
CREATE TRIGGER http_context_intent_delete_guard BEFORE DELETE ON http_context_intent_reservations
BEGIN SELECT RAISE(ABORT,'HTTP context intent accounting is append-only'); END;
