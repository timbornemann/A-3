-- Plan 22 candidate only. This migration is not registered in the production
-- migration list; production CURRENT remains V42 until the P11 cutover.
ALTER TABLE goal_contract_revisions ADD COLUMN original_contract_version INTEGER
  CHECK (original_contract_version IS NULL OR original_contract_version = 1);

CREATE TABLE goal_original_headers (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  original_digest BLOB NOT NULL CHECK (length(original_digest) = 32),
  byte_length INTEGER NOT NULL CHECK (byte_length BETWEEN 1 AND 65536),
  part_count INTEGER NOT NULL CHECK (part_count BETWEEN 1 AND 33),
  PRIMARY KEY (task_id, goal_revision),
  UNIQUE (task_id, goal_revision, original_digest),
  FOREIGN KEY (task_id, goal_revision)
    REFERENCES goal_contract_revisions(task_id, revision)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TABLE goal_original_parts (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  part_sequence INTEGER NOT NULL CHECK (part_sequence BETWEEN 1 AND 33),
  start_byte INTEGER NOT NULL CHECK (start_byte BETWEEN 0 AND 65535),
  end_byte INTEGER NOT NULL CHECK (end_byte BETWEEN 1 AND 65536),
  original_bytes BLOB NOT NULL CHECK (length(original_bytes) BETWEEN 1 AND 2048),
  CHECK (end_byte > start_byte AND length(original_bytes) = end_byte - start_byte),
  PRIMARY KEY (task_id, goal_revision, part_sequence),
  UNIQUE (task_id, goal_revision, start_byte),
  FOREIGN KEY (task_id, goal_revision)
    REFERENCES goal_original_headers(task_id, goal_revision)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TABLE goal_criterion_origins (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  criterion_id BLOB NOT NULL CHECK (length(criterion_id) = 32),
  original_digest BLOB NOT NULL CHECK (length(original_digest) = 32),
  start_byte INTEGER NOT NULL CHECK (start_byte BETWEEN 0 AND 65535),
  end_byte INTEGER NOT NULL CHECK (end_byte BETWEEN 1 AND 65536),
  requirement_kind TEXT NOT NULL
    CHECK (requirement_kind IN ('existing_test', 'change_only', 'runtime_needs_recipe')),
  command_id BLOB CHECK (command_id IS NULL OR length(command_id) = 32),
  selector_kind TEXT CHECK (selector_kind IS NULL OR selector_kind IN ('all', 'exact')),
  selector_name TEXT CHECK (selector_name IS NULL OR
    length(CAST(selector_name AS BLOB)) BETWEEN 1 AND 1024),
  minimum_cases INTEGER CHECK (minimum_cases IS NULL OR minimum_cases BETWEEN 1 AND 1000000),
  verification_scope TEXT CHECK (verification_scope IS NULL OR
    verification_scope IN ('targeted', 'package', 'workspace')),
  diff_mode TEXT CHECK (diff_mode IS NULL OR
    diff_mode IN ('no_changes', 'only_paths', 'exact_paths', 'non_empty_changes')),
  diff_path_count INTEGER NOT NULL CHECK (diff_path_count BETWEEN 0 AND 64),
  CHECK (end_byte > start_byte),
  CHECK (
    (requirement_kind = 'existing_test' AND command_id IS NOT NULL
      AND selector_kind IS NOT NULL AND minimum_cases IS NOT NULL
      AND verification_scope IS NOT NULL AND diff_mode IS NULL AND diff_path_count = 0
      AND ((selector_kind = 'all' AND selector_name IS NULL)
        OR (selector_kind = 'exact' AND selector_name IS NOT NULL)))
    OR (requirement_kind = 'change_only' AND command_id IS NULL
      AND selector_kind IS NULL AND selector_name IS NULL AND minimum_cases IS NULL
      AND verification_scope IS NULL AND diff_mode IS NOT NULL
      AND ((diff_mode IN ('no_changes', 'non_empty_changes') AND diff_path_count = 0)
        OR (diff_mode IN ('only_paths', 'exact_paths') AND diff_path_count BETWEEN 1 AND 64)))
    OR (requirement_kind = 'runtime_needs_recipe' AND command_id IS NULL
      AND selector_kind IS NULL AND selector_name IS NULL AND minimum_cases IS NULL
      AND verification_scope IS NULL AND diff_mode IS NULL AND diff_path_count = 0)
  ),
  PRIMARY KEY (task_id, goal_revision, criterion_id),
  FOREIGN KEY (task_id, goal_revision, criterion_id)
    REFERENCES acceptance_criteria(task_id, goal_revision, criterion_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (task_id, goal_revision, original_digest)
    REFERENCES goal_original_headers(task_id, goal_revision, original_digest)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TABLE goal_criterion_diff_paths (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  criterion_id BLOB NOT NULL CHECK (length(criterion_id) = 32),
  path_sequence INTEGER NOT NULL CHECK (path_sequence BETWEEN 1 AND 64),
  path BLOB NOT NULL CHECK (length(path) BETWEEN 1 AND 131072),
  PRIMARY KEY (task_id, goal_revision, criterion_id, path_sequence),
  UNIQUE (task_id, goal_revision, criterion_id, path),
  FOREIGN KEY (task_id, goal_revision, criterion_id)
    REFERENCES goal_criterion_origins(task_id, goal_revision, criterion_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

-- Immutable revision data must not be reinterpreted by an otherwise well-formed
-- UPDATE. Presentation deletion never owns these records.
CREATE TRIGGER goal_original_version_update_guard
BEFORE UPDATE OF original_contract_version ON goal_contract_revisions
WHEN OLD.original_contract_version IS NOT NEW.original_contract_version
BEGIN SELECT RAISE(ABORT, 'goal original version is immutable'); END;
CREATE TRIGGER goal_original_headers_update_guard BEFORE UPDATE ON goal_original_headers
BEGIN SELECT RAISE(ABORT, 'goal original header is immutable'); END;
CREATE TRIGGER goal_original_headers_delete_guard BEFORE DELETE ON goal_original_headers
BEGIN SELECT RAISE(ABORT, 'goal original header is immutable'); END;
CREATE TRIGGER goal_original_parts_update_guard BEFORE UPDATE ON goal_original_parts
BEGIN SELECT RAISE(ABORT, 'goal original parts are immutable'); END;
CREATE TRIGGER goal_original_parts_delete_guard BEFORE DELETE ON goal_original_parts
BEGIN SELECT RAISE(ABORT, 'goal original parts are immutable'); END;
CREATE TRIGGER goal_criterion_origins_update_guard BEFORE UPDATE ON goal_criterion_origins
BEGIN SELECT RAISE(ABORT, 'criterion original proof is immutable'); END;
CREATE TRIGGER goal_criterion_origins_delete_guard BEFORE DELETE ON goal_criterion_origins
BEGIN SELECT RAISE(ABORT, 'criterion original proof is immutable'); END;
CREATE TRIGGER goal_criterion_diff_paths_update_guard BEFORE UPDATE ON goal_criterion_diff_paths
BEGIN SELECT RAISE(ABORT, 'criterion diff paths are immutable'); END;
CREATE TRIGGER goal_criterion_diff_paths_delete_guard BEFORE DELETE ON goal_criterion_diff_paths
BEGIN SELECT RAISE(ABORT, 'criterion diff paths are immutable'); END;

CREATE TRIGGER goal_original_headers_insert_guard BEFORE INSERT ON goal_original_headers
WHEN (SELECT original_contract_version FROM goal_contract_revisions
  WHERE task_id = NEW.task_id AND revision = NEW.goal_revision) IS NOT 1
BEGIN SELECT RAISE(ABORT, 'goal original requires its declared revision'); END;
CREATE TRIGGER goal_original_parts_insert_guard BEFORE INSERT ON goal_original_parts
WHEN NOT EXISTS (SELECT 1 FROM goal_original_headers h
  WHERE h.task_id = NEW.task_id AND h.goal_revision = NEW.goal_revision
    AND NEW.part_sequence <= h.part_count)
BEGIN SELECT RAISE(ABORT, 'goal original part exceeds its declared count'); END;
CREATE TRIGGER goal_criterion_diff_paths_insert_guard BEFORE INSERT ON goal_criterion_diff_paths
WHEN NOT EXISTS (SELECT 1 FROM goal_criterion_origins o
  WHERE o.task_id = NEW.task_id AND o.goal_revision = NEW.goal_revision
    AND o.criterion_id = NEW.criterion_id AND o.requirement_kind = 'change_only'
    AND NEW.path_sequence <= o.diff_path_count)
BEGIN SELECT RAISE(ABORT, 'criterion diff path exceeds its declared proof'); END;

-- The original header is inserted after all existing Goal metadata. Its presence
-- closes that immutable revision to further metadata inserts as well as edits.
CREATE TRIGGER source_goal_revision_update_guard BEFORE UPDATE ON goal_contract_revisions
WHEN OLD.original_contract_version = 1
BEGIN SELECT RAISE(ABORT, 'original-backed goal revision is immutable'); END;
CREATE TRIGGER source_goal_revision_delete_guard BEFORE DELETE ON goal_contract_revisions
WHEN OLD.original_contract_version = 1
BEGIN SELECT RAISE(ABORT, 'original-backed goal revision is immutable'); END;
CREATE TRIGGER source_criteria_insert_guard BEFORE INSERT ON acceptance_criteria
WHEN EXISTS (SELECT 1 FROM goal_original_headers
  WHERE task_id = NEW.task_id AND goal_revision = NEW.goal_revision)
BEGIN SELECT RAISE(ABORT, 'original-backed criteria are closed'); END;
CREATE TRIGGER source_criteria_update_guard BEFORE UPDATE ON acceptance_criteria
WHEN EXISTS (SELECT 1 FROM goal_contract_revisions WHERE original_contract_version = 1
  AND ((task_id = OLD.task_id AND revision = OLD.goal_revision)
    OR (task_id = NEW.task_id AND revision = NEW.goal_revision)))
BEGIN SELECT RAISE(ABORT, 'original-backed criteria are immutable'); END;
CREATE TRIGGER source_criteria_delete_guard BEFORE DELETE ON acceptance_criteria
WHEN EXISTS (SELECT 1 FROM goal_contract_revisions WHERE original_contract_version = 1
  AND task_id = OLD.task_id AND revision = OLD.goal_revision)
BEGIN SELECT RAISE(ABORT, 'original-backed criteria are immutable'); END;

CREATE TRIGGER source_constraints_insert_guard BEFORE INSERT ON goal_contract_constraints
WHEN EXISTS (SELECT 1 FROM goal_original_headers
  WHERE task_id = NEW.task_id AND goal_revision = NEW.goal_revision)
BEGIN SELECT RAISE(ABORT, 'original-backed constraints are closed'); END;
CREATE TRIGGER source_constraints_update_guard BEFORE UPDATE ON goal_contract_constraints
WHEN EXISTS (SELECT 1 FROM goal_contract_revisions WHERE original_contract_version = 1
  AND ((task_id = OLD.task_id AND revision = OLD.goal_revision)
    OR (task_id = NEW.task_id AND revision = NEW.goal_revision)))
BEGIN SELECT RAISE(ABORT, 'original-backed constraints are immutable'); END;
CREATE TRIGGER source_constraints_delete_guard BEFORE DELETE ON goal_contract_constraints
WHEN EXISTS (SELECT 1 FROM goal_contract_revisions WHERE original_contract_version = 1
  AND task_id = OLD.task_id AND revision = OLD.goal_revision)
BEGIN SELECT RAISE(ABORT, 'original-backed constraints are immutable'); END;

CREATE TRIGGER source_non_goals_insert_guard BEFORE INSERT ON goal_contract_non_goals
WHEN EXISTS (SELECT 1 FROM goal_original_headers
  WHERE task_id = NEW.task_id AND goal_revision = NEW.goal_revision)
BEGIN SELECT RAISE(ABORT, 'original-backed non-goals are closed'); END;
CREATE TRIGGER source_non_goals_update_guard BEFORE UPDATE ON goal_contract_non_goals
WHEN EXISTS (SELECT 1 FROM goal_contract_revisions WHERE original_contract_version = 1
  AND ((task_id = OLD.task_id AND revision = OLD.goal_revision)
    OR (task_id = NEW.task_id AND revision = NEW.goal_revision)))
BEGIN SELECT RAISE(ABORT, 'original-backed non-goals are immutable'); END;
CREATE TRIGGER source_non_goals_delete_guard BEFORE DELETE ON goal_contract_non_goals
WHEN EXISTS (SELECT 1 FROM goal_contract_revisions WHERE original_contract_version = 1
  AND task_id = OLD.task_id AND revision = OLD.goal_revision)
BEGIN SELECT RAISE(ABORT, 'original-backed non-goals are immutable'); END;

CREATE TRIGGER source_user_decisions_insert_guard BEFORE INSERT ON goal_contract_user_decisions
WHEN EXISTS (SELECT 1 FROM goal_original_headers
  WHERE task_id = NEW.task_id AND goal_revision = NEW.goal_revision)
BEGIN SELECT RAISE(ABORT, 'original-backed user decisions are closed'); END;
CREATE TRIGGER source_user_decisions_update_guard BEFORE UPDATE ON goal_contract_user_decisions
WHEN EXISTS (SELECT 1 FROM goal_contract_revisions WHERE original_contract_version = 1
  AND ((task_id = OLD.task_id AND revision = OLD.goal_revision)
    OR (task_id = NEW.task_id AND revision = NEW.goal_revision)))
BEGIN SELECT RAISE(ABORT, 'original-backed user decisions are immutable'); END;
CREATE TRIGGER source_user_decisions_delete_guard BEFORE DELETE ON goal_contract_user_decisions
WHEN EXISTS (SELECT 1 FROM goal_contract_revisions WHERE original_contract_version = 1
  AND task_id = OLD.task_id AND revision = OLD.goal_revision)
BEGIN SELECT RAISE(ABORT, 'original-backed user decisions are immutable'); END;
