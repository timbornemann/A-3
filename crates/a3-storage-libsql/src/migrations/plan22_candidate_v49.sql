CREATE TABLE edit_phase_reservations (
    worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
    run_id BLOB NOT NULL CHECK (length(run_id) = 32),
    job_digest BLOB NOT NULL CHECK (length(job_digest) = 32),
    call_kind TEXT NOT NULL CHECK (call_kind IN ('initial', 'structural_repair')),
    task_id BLOB NOT NULL CHECK (length(task_id) = 32),
    goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
    ledger_revision INTEGER NOT NULL CHECK (ledger_revision BETWEEN 1 AND 4294967295),
    ledger_store_version INTEGER NOT NULL CHECK (ledger_store_version >= 1),
    run_sequence INTEGER NOT NULL CHECK (run_sequence >= 1),
    step_id BLOB NOT NULL CHECK (length(step_id) = 32),
    attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 4294967295),
    verification_id BLOB NOT NULL CHECK (length(verification_id) = 32),
    snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),
    wire_digest BLOB NOT NULL CHECK (length(wire_digest) = 32),
    wire_bytes INTEGER NOT NULL CHECK (wire_bytes BETWEEN 1 AND 4294967295),
    prompt_reserve INTEGER NOT NULL CHECK (prompt_reserve BETWEEN 1 AND 4294967295),
    output_reserve INTEGER NOT NULL CHECK (output_reserve BETWEEN 1 AND 4294967295),
    reserved_at INTEGER NOT NULL CHECK (reserved_at >= 0),
    deadline INTEGER NOT NULL CHECK (deadline > reserved_at),
    outcome TEXT CHECK (outcome IN ('accepted_reply', 'rejected_reply', 'invalid_structure',
        'provider_failure', 'cancelled', 'unknown', 'stale', 'budget_overrun')),
    finished_at INTEGER CHECK (finished_at >= reserved_at),
    reported_prompt INTEGER CHECK (reported_prompt BETWEEN 0 AND 4294967295),
    reported_output INTEGER CHECK (reported_output BETWEEN 0 AND 4294967295),
    PRIMARY KEY (run_id, job_digest, call_kind),
    CHECK ((outcome IS NULL) = (finished_at IS NULL)),
    CHECK (outcome IS NOT NULL OR (reported_prompt IS NULL AND reported_output IS NULL)),
    FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id),
    FOREIGN KEY (run_id, run_sequence) REFERENCES run_events(run_id, event_sequence),
    FOREIGN KEY (task_id, goal_revision) REFERENCES goal_contract_revisions(task_id, revision),
    FOREIGN KEY (task_id, step_id, attempt_number)
        REFERENCES task_step_attempts(task_id, step_id, attempt_number)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)
) STRICT;

CREATE UNIQUE INDEX edit_phase_one_unobserved_per_run
    ON edit_phase_reservations(run_id) WHERE outcome IS NULL;

CREATE TRIGGER edit_phase_insert_unobserved
BEFORE INSERT ON edit_phase_reservations WHEN NEW.outcome IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'Edit reservation must precede observation');
END;

CREATE TRIGGER edit_phase_observe_once
BEFORE UPDATE ON edit_phase_reservations
WHEN OLD.outcome IS NOT NULL OR NEW.outcome IS NULL
  OR NEW.worktree_id != OLD.worktree_id OR NEW.run_id != OLD.run_id
  OR NEW.job_digest != OLD.job_digest OR NEW.call_kind != OLD.call_kind
  OR NEW.task_id != OLD.task_id OR NEW.goal_revision != OLD.goal_revision
  OR NEW.ledger_revision != OLD.ledger_revision
  OR NEW.ledger_store_version != OLD.ledger_store_version
  OR NEW.run_sequence != OLD.run_sequence OR NEW.step_id != OLD.step_id
  OR NEW.attempt_number != OLD.attempt_number OR NEW.verification_id != OLD.verification_id
  OR NEW.snapshot_id != OLD.snapshot_id OR NEW.wire_digest != OLD.wire_digest
  OR NEW.wire_bytes != OLD.wire_bytes OR NEW.prompt_reserve != OLD.prompt_reserve
  OR NEW.output_reserve != OLD.output_reserve OR NEW.reserved_at != OLD.reserved_at
  OR NEW.deadline != OLD.deadline
BEGIN
    SELECT RAISE(ABORT, 'Edit reservation is immutable');
END;

CREATE TRIGGER edit_phase_no_delete
BEFORE DELETE ON edit_phase_reservations
BEGIN
    SELECT RAISE(ABORT, 'Edit reservation cannot be refunded');
END;
