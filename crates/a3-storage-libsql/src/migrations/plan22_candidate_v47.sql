-- Fresh candidate setup only. This extends the existing Ledger/Journal authority.
ALTER TABLE task_ledger_replans ADD COLUMN revision_cause TEXT NOT NULL
  DEFAULT 'error_correction' CHECK (revision_cause IN ('error_correction','horizon_extension'));

CREATE TABLE task_ledger_pending_planning (
  task_id BLOB PRIMARY KEY NOT NULL CHECK (length(task_id)=32),
  goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),
  run_id BLOB NOT NULL CHECK (length(run_id)=32),
  base_ledger_revision INTEGER NOT NULL CHECK (base_ledger_revision BETWEEN 1 AND 4294967295),
  snapshot_id BLOB NOT NULL CHECK (length(snapshot_id)=32),
  requested_event_id BLOB NOT NULL CHECK (length(requested_event_id)=32),
  requested_sequence INTEGER NOT NULL CHECK (requested_sequence BETWEEN 2 AND 9223372036854775807),
  stage TEXT NOT NULL CHECK (stage IN ('needs_plan','plan_appended')),
  appended_revision INTEGER CHECK (appended_revision IS NULL OR appended_revision BETWEEN 2 AND 4294967295),
  added_step_count INTEGER NOT NULL CHECK (added_step_count BETWEEN 0 AND 3),
  CHECK ((stage='needs_plan' AND appended_revision IS NULL AND added_step_count=0)
    OR (stage='plan_appended' AND appended_revision=base_ledger_revision+1 AND added_step_count BETWEEN 1 AND 3)),
  FOREIGN KEY (task_id,goal_revision) REFERENCES task_ledgers(task_id,goal_revision)
    ON UPDATE RESTRICT ON DELETE CASCADE,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT,
  -- The existing atomic writer projects Ledger before appending the request event.
  FOREIGN KEY (run_id,requested_sequence) REFERENCES run_events(run_id,event_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY (requested_event_id) REFERENCES run_events(event_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE task_ledger_pending_planning_steps (
  task_id BLOB NOT NULL CHECK (length(task_id)=32),
  step_sequence INTEGER NOT NULL CHECK (step_sequence BETWEEN 1 AND 3),
  step_id BLOB NOT NULL CHECK (length(step_id)=32),
  PRIMARY KEY (task_id,step_sequence),
  UNIQUE (task_id,step_id),
  FOREIGN KEY (task_id) REFERENCES task_ledger_pending_planning(task_id)
    ON UPDATE RESTRICT ON DELETE CASCADE,
  FOREIGN KEY (task_id,step_id) REFERENCES task_steps(task_id,step_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;
