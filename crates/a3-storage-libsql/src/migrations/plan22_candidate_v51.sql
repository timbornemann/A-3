-- Fresh candidate51 initialization only. Reject nonempty setup before replacing the table.
CREATE TABLE intake_phase_setup_guard (
    row_count INTEGER NOT NULL CHECK (row_count = 0)
) STRICT;
INSERT INTO intake_phase_setup_guard SELECT COUNT(*) FROM intake_phase_reservations;
DROP TABLE intake_phase_setup_guard;
DROP TABLE intake_phase_reservations;

CREATE TABLE intake_phase_reservations (
    call_kind TEXT NOT NULL CHECK (call_kind IN ('initial', 'structural_repair')),
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
    PRIMARY KEY (task_id, review_revision, window_start, window_end, call_kind),
    UNIQUE (task_id, job_digest, call_kind),
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
  OR NEW.call_kind != OLD.call_kind
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

CREATE TRIGGER intake_phase_repair_requires_invalid_initial
BEFORE INSERT ON intake_phase_reservations
WHEN NEW.call_kind = 'structural_repair' AND NOT EXISTS (
    SELECT 1 FROM intake_phase_reservations initial
    WHERE initial.task_id = NEW.task_id
      AND initial.review_revision = NEW.review_revision
      AND initial.window_start = NEW.window_start AND initial.window_end = NEW.window_end
      AND initial.job_digest = NEW.job_digest AND initial.wire_digest != NEW.wire_digest
      AND initial.call_kind = 'initial' AND initial.outcome = 'invalid_structure'
      AND initial.finished_at <= NEW.reserved_at
)
BEGIN
    SELECT RAISE(ABORT, 'Intake repair requires the same structurally invalid initial job');
END;

-- A nearby range or an unrelated Review append cannot reopen the same original bytes.
CREATE TRIGGER intake_phase_initial_requires_novel_window
BEFORE INSERT ON intake_phase_reservations
WHEN NEW.call_kind = 'initial' AND EXISTS (
    SELECT 1 FROM intake_phase_reservations initial
    WHERE initial.task_id = NEW.task_id AND initial.call_kind = 'initial'
      AND initial.window_start < NEW.window_end AND NEW.window_start < initial.window_end
)
BEGIN
    SELECT RAISE(ABORT, 'Intake initial requires an unattempted original window');
END;
