-- Fresh candidate52 setup. Initial Plan consumes the immutable Intake allowance.
CREATE TABLE initial_plan_phase_reservations (
    task_id BLOB NOT NULL CHECK (length(task_id) = 32),
    call_kind TEXT NOT NULL CHECK (call_kind IN ('initial', 'structural_repair')),
    goal_revision INTEGER NOT NULL CHECK (goal_revision = 1),
    job_digest BLOB NOT NULL CHECK (length(job_digest) = 32),
    index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),
    snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),
    wire_digest BLOB NOT NULL CHECK (length(wire_digest) = 32),
    wire_bytes INTEGER NOT NULL CHECK (wire_bytes BETWEEN 1 AND 4294967295),
    prompt_reserve INTEGER NOT NULL CHECK (prompt_reserve BETWEEN 1 AND 4294967295),
    output_reserve INTEGER NOT NULL CHECK (output_reserve BETWEEN 1 AND 4294967295),
    reserved_at INTEGER NOT NULL CHECK (reserved_at >= 0),
    outcome TEXT CHECK (outcome IN ('accepted', 'need_evidence', 'too_large', 'rejected_reply',
        'invalid_structure', 'provider_failure', 'cancelled', 'unknown', 'stale', 'budget_overrun')),
    finished_at INTEGER CHECK (finished_at >= reserved_at),
    reported_prompt INTEGER CHECK (reported_prompt BETWEEN 0 AND 4294967295),
    reported_output INTEGER CHECK (reported_output BETWEEN 0 AND 4294967295),
    accepted_ledger_version INTEGER CHECK (accepted_ledger_version = 1),
    CHECK ((outcome IS NULL) = (finished_at IS NULL)),
    CHECK (outcome IS NOT NULL OR (reported_prompt IS NULL AND reported_output IS NULL)),
    CHECK (COALESCE(outcome = 'accepted', 0) = (accepted_ledger_version IS NOT NULL)),
    PRIMARY KEY (task_id, call_kind),
    FOREIGN KEY (task_id) REFERENCES intake_phase_budget_scopes(task_id),
    FOREIGN KEY (task_id, goal_revision) REFERENCES goal_contract_revisions(task_id, revision)
) STRICT;

CREATE UNIQUE INDEX initial_plan_one_unobserved_per_task
    ON initial_plan_phase_reservations(task_id) WHERE outcome IS NULL;

CREATE TRIGGER initial_plan_insert_unobserved
BEFORE INSERT ON initial_plan_phase_reservations WHEN NEW.outcome IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'Initial Plan reservation must precede observation');
END;

CREATE TRIGGER initial_plan_observe_once
BEFORE UPDATE ON initial_plan_phase_reservations
WHEN OLD.outcome IS NOT NULL OR NEW.outcome IS NULL
  OR NEW.task_id != OLD.task_id OR NEW.call_kind != OLD.call_kind
  OR NEW.goal_revision != OLD.goal_revision OR NEW.job_digest != OLD.job_digest
  OR NEW.index_run_id != OLD.index_run_id OR NEW.snapshot_id != OLD.snapshot_id
  OR NEW.wire_digest != OLD.wire_digest OR NEW.wire_bytes != OLD.wire_bytes
  OR NEW.prompt_reserve != OLD.prompt_reserve OR NEW.output_reserve != OLD.output_reserve
  OR NEW.reserved_at != OLD.reserved_at
BEGIN
    SELECT RAISE(ABORT, 'Initial Plan reservation is immutable');
END;

CREATE TRIGGER initial_plan_no_delete
BEFORE DELETE ON initial_plan_phase_reservations
BEGIN
    SELECT RAISE(ABORT, 'Initial Plan reservation cannot be refunded');
END;
