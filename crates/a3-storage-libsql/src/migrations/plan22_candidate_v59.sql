-- The model-selected Add scope is a plan operation, never an original-user fact.
ALTER TABLE task_steps ADD COLUMN has_planned_file_addition INTEGER NOT NULL DEFAULT 0
    CHECK (has_planned_file_addition IN (0, 1));

CREATE TABLE task_step_planned_file_additions (
    task_id BLOB NOT NULL CHECK (length(task_id) = 32),
    step_id BLOB NOT NULL CHECK (length(step_id) = 32),
    target_path BLOB NOT NULL CHECK (length(target_path) BETWEEN 1 AND 131072),
    PRIMARY KEY (task_id, step_id),
    FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)
        ON UPDATE RESTRICT ON DELETE CASCADE
) STRICT;

-- A semantic model halt is an observed, charged reply, never a repairable decode error.
ALTER TABLE initial_plan_phase_reservations
    ADD COLUMN has_semantic_halt INTEGER NOT NULL DEFAULT 0
    CHECK (has_semantic_halt IN (0, 1)
        AND (has_semantic_halt = 0 OR outcome IS NOT NULL));

CREATE TABLE initial_plan_phase_halts (
    task_id BLOB NOT NULL CHECK (length(task_id) = 32),
    call_kind TEXT NOT NULL CHECK (call_kind IN ('initial', 'structural_repair')),
    reason TEXT NOT NULL CHECK (reason IN ('refuse', 'clarify')),
    PRIMARY KEY (task_id, call_kind),
    FOREIGN KEY (task_id, call_kind)
        REFERENCES initial_plan_phase_reservations(task_id, call_kind)
        ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER initial_plan_halt_requires_marker
BEFORE INSERT ON initial_plan_phase_halts
WHEN NOT EXISTS (
    SELECT 1 FROM initial_plan_phase_reservations
    WHERE task_id = NEW.task_id AND call_kind = NEW.call_kind AND has_semantic_halt = 1
)
BEGIN
    SELECT RAISE(ABORT, 'Initial Plan halt requires its settled reservation');
END;

CREATE TRIGGER initial_plan_halt_no_update
BEFORE UPDATE ON initial_plan_phase_halts
BEGIN
    SELECT RAISE(ABORT, 'Initial Plan halt is immutable');
END;

CREATE TRIGGER initial_plan_halt_no_delete
BEFORE DELETE ON initial_plan_phase_halts
BEGIN
    SELECT RAISE(ABORT, 'Initial Plan halt cannot be removed');
END;
