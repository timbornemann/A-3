-- Plan 22 candidate only. Production CURRENT and migration list remain V42.
-- Optional entry provenance is immutable within one Goal revision. The existing
-- original header seals the revision, so origins are inserted before that header
-- in the same Goal transaction and its foreign key is checked at COMMIT.

CREATE TABLE goal_constraint_origins (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),
  original_digest BLOB NOT NULL CHECK (length(original_digest) = 32),
  start_byte INTEGER NOT NULL CHECK (start_byte BETWEEN 0 AND 65535),
  end_byte INTEGER NOT NULL CHECK (end_byte BETWEEN 1 AND 65536),
  CHECK (end_byte > start_byte),
  PRIMARY KEY (task_id, goal_revision, item_sequence),
  FOREIGN KEY (task_id, goal_revision, item_sequence)
    REFERENCES goal_contract_constraints(task_id, goal_revision, item_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (task_id, goal_revision, original_digest)
    REFERENCES goal_original_headers(task_id, goal_revision, original_digest)
    ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TRIGGER goal_constraint_origins_insert_guard BEFORE INSERT ON goal_constraint_origins
WHEN EXISTS (SELECT 1 FROM goal_original_headers
  WHERE task_id = NEW.task_id AND goal_revision = NEW.goal_revision)
BEGIN SELECT RAISE(ABORT, 'constraint origins are closed'); END;
CREATE TRIGGER goal_constraint_origins_update_guard BEFORE UPDATE ON goal_constraint_origins
BEGIN SELECT RAISE(ABORT, 'constraint origin is immutable'); END;
CREATE TRIGGER goal_constraint_origins_delete_guard BEFORE DELETE ON goal_constraint_origins
BEGIN SELECT RAISE(ABORT, 'constraint origin is immutable'); END;

CREATE TABLE goal_non_goal_origins (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),
  original_digest BLOB NOT NULL CHECK (length(original_digest) = 32),
  start_byte INTEGER NOT NULL CHECK (start_byte BETWEEN 0 AND 65535),
  end_byte INTEGER NOT NULL CHECK (end_byte BETWEEN 1 AND 65536),
  CHECK (end_byte > start_byte),
  PRIMARY KEY (task_id, goal_revision, item_sequence),
  FOREIGN KEY (task_id, goal_revision, item_sequence)
    REFERENCES goal_contract_non_goals(task_id, goal_revision, item_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (task_id, goal_revision, original_digest)
    REFERENCES goal_original_headers(task_id, goal_revision, original_digest)
    ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TRIGGER goal_non_goal_origins_insert_guard BEFORE INSERT ON goal_non_goal_origins
WHEN EXISTS (SELECT 1 FROM goal_original_headers
  WHERE task_id = NEW.task_id AND goal_revision = NEW.goal_revision)
BEGIN SELECT RAISE(ABORT, 'non-goal origins are closed'); END;
CREATE TRIGGER goal_non_goal_origins_update_guard BEFORE UPDATE ON goal_non_goal_origins
BEGIN SELECT RAISE(ABORT, 'non-goal origin is immutable'); END;
CREATE TRIGGER goal_non_goal_origins_delete_guard BEFORE DELETE ON goal_non_goal_origins
BEGIN SELECT RAISE(ABORT, 'non-goal origin is immutable'); END;

CREATE TABLE goal_user_decision_origins (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),
  original_digest BLOB NOT NULL CHECK (length(original_digest) = 32),
  start_byte INTEGER NOT NULL CHECK (start_byte BETWEEN 0 AND 65535),
  end_byte INTEGER NOT NULL CHECK (end_byte BETWEEN 1 AND 65536),
  CHECK (end_byte > start_byte),
  PRIMARY KEY (task_id, goal_revision, item_sequence),
  FOREIGN KEY (task_id, goal_revision, item_sequence)
    REFERENCES goal_contract_user_decisions(task_id, goal_revision, item_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (task_id, goal_revision, original_digest)
    REFERENCES goal_original_headers(task_id, goal_revision, original_digest)
    ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TRIGGER goal_user_decision_origins_insert_guard BEFORE INSERT ON goal_user_decision_origins
WHEN EXISTS (SELECT 1 FROM goal_original_headers
  WHERE task_id = NEW.task_id AND goal_revision = NEW.goal_revision)
BEGIN SELECT RAISE(ABORT, 'user decision origins are closed'); END;
CREATE TRIGGER goal_user_decision_origins_update_guard BEFORE UPDATE ON goal_user_decision_origins
BEGIN SELECT RAISE(ABORT, 'user decision origin is immutable'); END;
CREATE TRIGGER goal_user_decision_origins_delete_guard BEFORE DELETE ON goal_user_decision_origins
BEGIN SELECT RAISE(ABORT, 'user decision origin is immutable'); END;
