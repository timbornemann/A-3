-- Fresh candidate50 setup. A budget survives promotion without duplicating its original.
CREATE TABLE intake_phase_budget_scopes (
    task_id BLOB PRIMARY KEY NOT NULL CHECK (length(task_id) = 32),
    worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
    original_digest BLOB NOT NULL CHECK (length(original_digest) = 32),
    capture_version INTEGER NOT NULL CHECK (capture_version >= 1),
    captured_at INTEGER NOT NULL CHECK (captured_at >= 0),
    model_profile_id BLOB NOT NULL CHECK (length(model_profile_id) = 32),
    model_profile_version INTEGER NOT NULL CHECK (model_profile_version >= 1),
    turn_limit INTEGER NOT NULL CHECK (turn_limit >= 1),
    prompt_limit INTEGER NOT NULL CHECK (prompt_limit >= 1),
    output_limit INTEGER NOT NULL CHECK (output_limit >= 1),
    action_limit INTEGER NOT NULL CHECK (action_limit >= 1),
    repair_limit INTEGER NOT NULL CHECK (repair_limit >= 1),
    deadline INTEGER NOT NULL CHECK (deadline > captured_at),
    linked_run_id BLOB UNIQUE CHECK (linked_run_id IS NULL OR length(linked_run_id) = 32),
    FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id),
    FOREIGN KEY (linked_run_id) REFERENCES agent_runs(run_id)
) STRICT;

CREATE TRIGGER intake_scope_requires_capture
BEFORE INSERT ON intake_phase_budget_scopes
WHEN NEW.linked_run_id IS NOT NULL OR NOT EXISTS (
    SELECT 1 FROM goal_pending_intakes p
    WHERE p.task_id = NEW.task_id AND p.worktree_id = NEW.worktree_id
      AND p.original_digest = NEW.original_digest AND p.store_version = NEW.capture_version
      AND p.created_at_unix_millis = NEW.captured_at
)
BEGIN
    SELECT RAISE(ABORT, 'Intake scope requires its exact Pending capture');
END;

CREATE TRIGGER intake_scope_link_once
BEFORE UPDATE ON intake_phase_budget_scopes
WHEN OLD.linked_run_id IS NOT NULL OR NEW.linked_run_id IS NULL
  OR NEW.task_id != OLD.task_id OR NEW.worktree_id != OLD.worktree_id
  OR NEW.original_digest != OLD.original_digest OR NEW.capture_version != OLD.capture_version
  OR NEW.captured_at != OLD.captured_at OR NEW.model_profile_id != OLD.model_profile_id
  OR NEW.model_profile_version != OLD.model_profile_version OR NEW.turn_limit != OLD.turn_limit
  OR NEW.prompt_limit != OLD.prompt_limit OR NEW.output_limit != OLD.output_limit
  OR NEW.action_limit != OLD.action_limit OR NEW.repair_limit != OLD.repair_limit
  OR NEW.deadline != OLD.deadline
BEGIN
    SELECT RAISE(ABORT, 'Intake allowance is immutable');
END;

CREATE TABLE intake_phase_reservations (
    task_id BLOB NOT NULL CHECK (length(task_id) = 32),
    review_revision INTEGER NOT NULL CHECK (review_revision BETWEEN 0 AND 128),
    window_start INTEGER NOT NULL CHECK (window_start BETWEEN 0 AND 65535),
    window_end INTEGER NOT NULL CHECK (window_end BETWEEN 1 AND 65536),
    job_digest BLOB NOT NULL CHECK (length(job_digest) = 32),
    wire_digest BLOB NOT NULL CHECK (length(wire_digest) = 32),
    wire_bytes INTEGER NOT NULL CHECK (wire_bytes BETWEEN 1 AND 4294967295),
    prompt_reserve INTEGER NOT NULL CHECK (prompt_reserve BETWEEN 1 AND 4294967295),
    output_reserve INTEGER NOT NULL CHECK (output_reserve BETWEEN 1 AND 4294967295),
    reserved_at INTEGER NOT NULL CHECK (reserved_at >= 0),
    outcome TEXT CHECK (outcome IN ('accepted_review', 'rejected_reply', 'invalid_structure',
        'provider_failure', 'cancelled', 'unknown', 'stale', 'budget_overrun')),
    finished_at INTEGER CHECK (finished_at >= reserved_at),
    reported_prompt INTEGER CHECK (reported_prompt BETWEEN 0 AND 4294967295),
    reported_output INTEGER CHECK (reported_output BETWEEN 0 AND 4294967295),
    CHECK (window_end > window_start AND window_end - window_start <= 2048),
    CHECK ((outcome IS NULL) = (finished_at IS NULL)),
    CHECK (outcome IS NOT NULL OR (reported_prompt IS NULL AND reported_output IS NULL)),
    PRIMARY KEY (task_id, review_revision, window_start, window_end),
    UNIQUE (task_id, job_digest),
    FOREIGN KEY (task_id) REFERENCES intake_phase_budget_scopes(task_id)
) STRICT;

CREATE UNIQUE INDEX intake_phase_one_unobserved_per_task
    ON intake_phase_reservations(task_id) WHERE outcome IS NULL;

CREATE TRIGGER intake_phase_insert_unobserved
BEFORE INSERT ON intake_phase_reservations WHEN NEW.outcome IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'Intake reservation must precede observation');
END;

CREATE TRIGGER intake_phase_observe_once
BEFORE UPDATE ON intake_phase_reservations
WHEN OLD.outcome IS NOT NULL OR NEW.outcome IS NULL
  OR NEW.task_id != OLD.task_id OR NEW.review_revision != OLD.review_revision
  OR NEW.window_start != OLD.window_start OR NEW.window_end != OLD.window_end
  OR NEW.job_digest != OLD.job_digest OR NEW.wire_digest != OLD.wire_digest
  OR NEW.wire_bytes != OLD.wire_bytes OR NEW.prompt_reserve != OLD.prompt_reserve
  OR NEW.output_reserve != OLD.output_reserve OR NEW.reserved_at != OLD.reserved_at
BEGIN
    SELECT RAISE(ABORT, 'Intake reservation is immutable');
END;

CREATE TRIGGER intake_phase_no_delete
BEFORE DELETE ON intake_phase_reservations
BEGIN
    SELECT RAISE(ABORT, 'Intake reservation cannot be refunded');
END;
