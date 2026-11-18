-- Fresh candidate48 setup only. Reviews contain proposals, never another raw source.
CREATE TABLE goal_intake_review_batches (
  task_id BLOB NOT NULL CHECK(length(task_id)=32),
  review_sequence INTEGER NOT NULL CHECK(review_sequence BETWEEN 1 AND 128),
  worktree_id BLOB NOT NULL CHECK(length(worktree_id)=32),
  original_digest BLOB NOT NULL CHECK(length(original_digest)=32),
  capture_created_at INTEGER NOT NULL CHECK(capture_created_at>=0),
  window_start INTEGER NOT NULL CHECK(window_start BETWEEN 0 AND 65535),
  window_end INTEGER NOT NULL CHECK(window_end BETWEEN 1 AND 65536),
  finding_count INTEGER NOT NULL CHECK(finding_count BETWEEN 0 AND 4),
  question_count INTEGER NOT NULL CHECK(question_count BETWEEN 0 AND 1),
  CHECK(window_end>window_start AND window_end-window_start<=2048),
  PRIMARY KEY(task_id,review_sequence),
  FOREIGN KEY(worktree_id) REFERENCES worktrees(worktree_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TABLE goal_intake_review_findings (
  task_id BLOB NOT NULL CHECK(length(task_id)=32),
  review_sequence INTEGER NOT NULL CHECK(review_sequence BETWEEN 1 AND 128),
  finding_sequence INTEGER NOT NULL CHECK(finding_sequence BETWEEN 1 AND 4),
  kind TEXT NOT NULL CHECK(kind IN ('outcome','constraint','non_goal','existing_decision')),
  start_byte INTEGER NOT NULL CHECK(start_byte BETWEEN 0 AND 65535),
  end_byte INTEGER NOT NULL CHECK(end_byte BETWEEN 1 AND 65536 AND end_byte>start_byte),
  statement TEXT NOT NULL CHECK(length(CAST(statement AS BLOB)) BETWEEN 1 AND 2048),
  requiredness TEXT CHECK(requiredness IS NULL OR requiredness IN ('must','should')),
  evidence_kind TEXT CHECK(evidence_kind IS NULL OR evidence_kind IN ('existing_test','change_only','runtime_needs_recipe')),
  command_id BLOB CHECK(command_id IS NULL OR length(command_id)=32),
  selector_kind TEXT CHECK(selector_kind IS NULL OR selector_kind IN ('all','exact')),
  selector_name TEXT CHECK(selector_name IS NULL OR length(CAST(selector_name AS BLOB)) BETWEEN 1 AND 1024),
  minimum_cases INTEGER CHECK(minimum_cases IS NULL OR minimum_cases BETWEEN 1 AND 1000000),
  verification_scope TEXT CHECK(verification_scope IS NULL OR verification_scope IN ('targeted','package','workspace')),
  diff_mode TEXT CHECK(diff_mode IS NULL OR diff_mode IN ('no_changes','only_paths','exact_paths','non_empty_changes')),
  diff_path_count INTEGER NOT NULL CHECK(diff_path_count BETWEEN 0 AND 64),
  CHECK((kind='outcome' AND requiredness IS NOT NULL AND evidence_kind IS NOT NULL)
    OR (kind!='outcome' AND requiredness IS NULL AND evidence_kind IS NULL
      AND command_id IS NULL AND selector_kind IS NULL AND selector_name IS NULL
      AND minimum_cases IS NULL AND verification_scope IS NULL AND diff_mode IS NULL AND diff_path_count=0)),
  CHECK(evidence_kind IS NULL OR
    (evidence_kind='existing_test' AND command_id IS NOT NULL AND selector_kind IS NOT NULL
      AND minimum_cases IS NOT NULL AND verification_scope IS NOT NULL AND diff_mode IS NULL AND diff_path_count=0
      AND ((selector_kind='all' AND selector_name IS NULL) OR (selector_kind='exact' AND selector_name IS NOT NULL))) OR
    (evidence_kind='change_only' AND command_id IS NULL AND selector_kind IS NULL AND selector_name IS NULL
      AND minimum_cases IS NULL AND verification_scope IS NULL AND diff_mode IS NOT NULL
      AND ((diff_mode IN ('no_changes','non_empty_changes') AND diff_path_count=0)
        OR (diff_mode IN ('only_paths','exact_paths') AND diff_path_count BETWEEN 1 AND 64))) OR
    (evidence_kind='runtime_needs_recipe' AND command_id IS NULL AND selector_kind IS NULL AND selector_name IS NULL
      AND minimum_cases IS NULL AND verification_scope IS NULL AND diff_mode IS NULL AND diff_path_count=0)),
  PRIMARY KEY(task_id,review_sequence,finding_sequence),
  FOREIGN KEY(task_id,review_sequence) REFERENCES goal_intake_review_batches(task_id,review_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE goal_intake_review_diff_paths (
  task_id BLOB NOT NULL CHECK(length(task_id)=32),
  review_sequence INTEGER NOT NULL CHECK(review_sequence BETWEEN 1 AND 128),
  finding_sequence INTEGER NOT NULL CHECK(finding_sequence BETWEEN 1 AND 4),
  path_sequence INTEGER NOT NULL CHECK(path_sequence BETWEEN 1 AND 64),
  path BLOB NOT NULL CHECK(length(path) BETWEEN 1 AND 65536),
  PRIMARY KEY(task_id,review_sequence,finding_sequence,path_sequence),
  UNIQUE(task_id,review_sequence,finding_sequence,path),
  FOREIGN KEY(task_id,review_sequence,finding_sequence)
    REFERENCES goal_intake_review_findings(task_id,review_sequence,finding_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TABLE goal_intake_review_questions (
  task_id BLOB NOT NULL CHECK(length(task_id)=32),
  review_sequence INTEGER NOT NULL CHECK(review_sequence BETWEEN 1 AND 128),
  question_id INTEGER NOT NULL CHECK(question_id BETWEEN 1 AND 32),
  kind TEXT NOT NULL CHECK(kind IN ('missing_user_choice','conflicting_requirements')),
  question TEXT NOT NULL CHECK(length(CAST(question AS BLOB)) BETWEEN 1 AND 512),
  range_count INTEGER NOT NULL CHECK(range_count BETWEEN 1 AND 8),
  PRIMARY KEY(task_id,review_sequence,question_id),
  UNIQUE(task_id,question_id),
  FOREIGN KEY(task_id,review_sequence) REFERENCES goal_intake_review_batches(task_id,review_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE goal_intake_review_question_ranges (
  task_id BLOB NOT NULL CHECK(length(task_id)=32),
  review_sequence INTEGER NOT NULL CHECK(review_sequence BETWEEN 1 AND 128),
  question_id INTEGER NOT NULL CHECK(question_id BETWEEN 1 AND 32),
  range_sequence INTEGER NOT NULL CHECK(range_sequence BETWEEN 1 AND 8),
  start_byte INTEGER NOT NULL CHECK(start_byte BETWEEN 0 AND 65535),
  end_byte INTEGER NOT NULL CHECK(end_byte BETWEEN 1 AND 65536 AND end_byte>start_byte),
  PRIMARY KEY(task_id,review_sequence,question_id,range_sequence),
  UNIQUE(task_id,review_sequence,question_id,start_byte,end_byte),
  FOREIGN KEY(task_id,review_sequence,question_id)
    REFERENCES goal_intake_review_questions(task_id,review_sequence,question_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

-- Header-last seals complete children in the same transaction and prevents late reinterpretation.
CREATE TRIGGER goal_intake_review_batches_insert_guard BEFORE INSERT ON goal_intake_review_batches
WHEN NOT EXISTS(SELECT 1 FROM goal_pending_intakes p WHERE p.task_id=NEW.task_id
    AND p.worktree_id=NEW.worktree_id AND p.original_digest=NEW.original_digest
    AND p.created_at_unix_millis=NEW.capture_created_at)
  OR NEW.review_sequence!=(SELECT coalesce(max(review_sequence),0)+1 FROM goal_intake_review_batches WHERE task_id=NEW.task_id)
  OR EXISTS(SELECT 1 FROM goal_intake_review_batches b WHERE b.task_id=NEW.task_id
    AND b.window_start<NEW.window_end AND NEW.window_start<b.window_end)
  OR (SELECT coalesce(sum(length(path)),0) FROM goal_intake_review_diff_paths WHERE task_id=NEW.task_id)>65536
  OR NEW.finding_count!=(SELECT count(*) FROM goal_intake_review_findings f WHERE f.task_id=NEW.task_id AND f.review_sequence=NEW.review_sequence)
  OR NEW.question_count!=(SELECT count(*) FROM goal_intake_review_questions q WHERE q.task_id=NEW.task_id AND q.review_sequence=NEW.review_sequence)
  OR EXISTS(SELECT 1 FROM goal_intake_review_findings f WHERE f.task_id=NEW.task_id AND f.review_sequence=NEW.review_sequence
    AND f.diff_path_count!=(SELECT count(*) FROM goal_intake_review_diff_paths p WHERE p.task_id=f.task_id AND p.review_sequence=f.review_sequence AND p.finding_sequence=f.finding_sequence))
  OR EXISTS(SELECT 1 FROM goal_intake_review_questions q WHERE q.task_id=NEW.task_id AND q.review_sequence=NEW.review_sequence
    AND q.range_count!=(SELECT count(*) FROM goal_intake_review_question_ranges r WHERE r.task_id=q.task_id AND r.review_sequence=q.review_sequence AND r.question_id=q.question_id))
BEGIN SELECT RAISE(ABORT,'review batch is not a complete successor of its captured original'); END;

CREATE TRIGGER goal_pending_intakes_review_insert_guard BEFORE INSERT ON goal_pending_intakes
WHEN EXISTS(SELECT 1 FROM goal_intake_review_batches WHERE task_id=NEW.task_id)
BEGIN SELECT RAISE(ABORT,'reviewed capture cannot be replaced'); END;
CREATE TRIGGER goal_intake_review_batches_update_guard BEFORE UPDATE ON goal_intake_review_batches
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_batches_delete_guard BEFORE DELETE ON goal_intake_review_batches
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_findings_update_guard BEFORE UPDATE ON goal_intake_review_findings
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_findings_delete_guard BEFORE DELETE ON goal_intake_review_findings
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_findings_insert_guard BEFORE INSERT ON goal_intake_review_findings
WHEN EXISTS(SELECT 1 FROM goal_intake_review_batches b WHERE b.task_id=NEW.task_id AND b.review_sequence=NEW.review_sequence)
BEGIN SELECT RAISE(ABORT,'intake review batch is sealed'); END;
CREATE TRIGGER goal_intake_review_diff_paths_update_guard BEFORE UPDATE ON goal_intake_review_diff_paths
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_diff_paths_delete_guard BEFORE DELETE ON goal_intake_review_diff_paths
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_diff_paths_insert_guard BEFORE INSERT ON goal_intake_review_diff_paths
WHEN EXISTS(SELECT 1 FROM goal_intake_review_batches b WHERE b.task_id=NEW.task_id AND b.review_sequence=NEW.review_sequence)
BEGIN SELECT RAISE(ABORT,'intake review batch is sealed'); END;
CREATE TRIGGER goal_intake_review_questions_update_guard BEFORE UPDATE ON goal_intake_review_questions
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_questions_delete_guard BEFORE DELETE ON goal_intake_review_questions
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_questions_insert_guard BEFORE INSERT ON goal_intake_review_questions
WHEN EXISTS(SELECT 1 FROM goal_intake_review_batches b WHERE b.task_id=NEW.task_id AND b.review_sequence=NEW.review_sequence)
BEGIN SELECT RAISE(ABORT,'intake review batch is sealed'); END;
CREATE TRIGGER goal_intake_review_question_ranges_update_guard BEFORE UPDATE ON goal_intake_review_question_ranges
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_question_ranges_delete_guard BEFORE DELETE ON goal_intake_review_question_ranges
BEGIN SELECT RAISE(ABORT,'intake review history is immutable'); END;
CREATE TRIGGER goal_intake_review_question_ranges_insert_guard BEFORE INSERT ON goal_intake_review_question_ranges
WHEN EXISTS(SELECT 1 FROM goal_intake_review_batches b WHERE b.task_id=NEW.task_id AND b.review_sequence=NEW.review_sequence)
BEGIN SELECT RAISE(ABORT,'intake review batch is sealed'); END;
