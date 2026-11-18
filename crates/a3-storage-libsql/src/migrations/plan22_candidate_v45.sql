-- Fresh Plan 22 candidate setup only; no adoption of an existing V44 database.
-- An input owns exact source bytes before there is a valid task/Goal Contract.
CREATE TABLE goal_pending_intakes (
  task_id BLOB PRIMARY KEY NOT NULL CHECK (length(task_id) = 32),
  worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
  original_digest BLOB NOT NULL CHECK (length(original_digest) = 32),
  byte_length INTEGER NOT NULL CHECK (byte_length BETWEEN 1 AND 65536),
  original_bytes BLOB NOT NULL CHECK (length(original_bytes) BETWEEN 1 AND 65536),
  created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),
  store_version INTEGER NOT NULL CHECK (store_version BETWEEN 1 AND 9223372036854775807),
  CHECK (length(original_bytes) = byte_length),
  FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER goal_pending_intakes_insert_guard BEFORE INSERT ON goal_pending_intakes
WHEN EXISTS (SELECT 1 FROM tasks WHERE task_id = NEW.task_id)
BEGIN SELECT RAISE(ABORT, 'task identity is already materialized'); END;
CREATE TRIGGER goal_pending_intakes_update_guard BEFORE UPDATE ON goal_pending_intakes
BEGIN SELECT RAISE(ABORT, 'captured original is immutable'); END;

-- Promotion consumes Pending by CAS before calling the normal Task/Goal writer
-- inside the same IMMEDIATE transaction. Failure restores Pending by rollback.
CREATE TRIGGER tasks_pending_intake_insert_guard BEFORE INSERT ON tasks
WHEN EXISTS (SELECT 1 FROM goal_pending_intakes WHERE task_id = NEW.task_id)
BEGIN SELECT RAISE(ABORT, 'pending intake must be consumed atomically'); END;
