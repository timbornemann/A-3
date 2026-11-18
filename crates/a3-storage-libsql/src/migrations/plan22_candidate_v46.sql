-- Fresh Plan 22 candidate setup only. The existing acceptance mapping remains Proves.
CREATE TABLE task_step_addressed_criteria (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  step_id BLOB NOT NULL CHECK (length(step_id) = 32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  criterion_id BLOB NOT NULL CHECK (length(criterion_id) = 32),
  PRIMARY KEY (task_id, step_id, criterion_id),
  FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)
    ON UPDATE RESTRICT ON DELETE CASCADE,
  FOREIGN KEY (task_id, goal_revision) REFERENCES task_ledgers(task_id, goal_revision)
    ON UPDATE RESTRICT ON DELETE CASCADE,
  FOREIGN KEY (task_id, goal_revision, criterion_id)
    REFERENCES acceptance_criteria(task_id, goal_revision, criterion_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER step_addresses_disjoint_insert BEFORE INSERT ON task_step_addressed_criteria
WHEN EXISTS (SELECT 1 FROM task_step_acceptance_criteria
  WHERE task_id=NEW.task_id AND step_id=NEW.step_id AND criterion_id=NEW.criterion_id)
BEGIN SELECT RAISE(ABORT, 'step criterion roles overlap'); END;
CREATE TRIGGER step_addresses_disjoint_update BEFORE UPDATE ON task_step_addressed_criteria
WHEN EXISTS (SELECT 1 FROM task_step_acceptance_criteria
  WHERE task_id=NEW.task_id AND step_id=NEW.step_id AND criterion_id=NEW.criterion_id)
BEGIN SELECT RAISE(ABORT, 'step criterion roles overlap'); END;
CREATE TRIGGER step_proves_disjoint_insert BEFORE INSERT ON task_step_acceptance_criteria
WHEN EXISTS (SELECT 1 FROM task_step_addressed_criteria
  WHERE task_id=NEW.task_id AND step_id=NEW.step_id AND criterion_id=NEW.criterion_id)
BEGIN SELECT RAISE(ABORT, 'step criterion roles overlap'); END;
CREATE TRIGGER step_proves_disjoint_update BEFORE UPDATE ON task_step_acceptance_criteria
WHEN EXISTS (SELECT 1 FROM task_step_addressed_criteria
  WHERE task_id=NEW.task_id AND step_id=NEW.step_id AND criterion_id=NEW.criterion_id)
BEGIN SELECT RAISE(ABORT, 'step criterion roles overlap'); END;
