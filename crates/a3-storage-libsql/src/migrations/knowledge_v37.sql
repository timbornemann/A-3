-- Shared research state owned by existing run events; no source text or executable task.
CREATE TABLE agent_replan_research_checkpoints (
  run_id BLOB NOT NULL CHECK (length(run_id) = 32),
  event_sequence INTEGER NOT NULL CHECK (event_sequence > 0),
  step_id BLOB NOT NULL CHECK (length(step_id) = 32),
  snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),
  payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) BETWEEN 1 AND 524288),
  PRIMARY KEY (run_id, event_sequence),
  FOREIGN KEY (run_id, event_sequence) REFERENCES run_events(run_id, event_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;
CREATE INDEX agent_replan_research_owner ON agent_replan_research_checkpoints(run_id, step_id, event_sequence DESC);
CREATE TRIGGER agent_replan_research_update_guard BEFORE UPDATE ON agent_replan_research_checkpoints
BEGIN SELECT RAISE(ABORT, 'replan research checkpoints are immutable'); END;
CREATE TRIGGER agent_replan_research_delete_guard BEFORE DELETE ON agent_replan_research_checkpoints
BEGIN SELECT RAISE(ABORT, 'replan research checkpoints are append-only'); END;

-- Only actual Safe Reader pages may be hydrated after restart; search spans cannot qualify.
CREATE TABLE agent_replan_originals (
  run_id BLOB NOT NULL,
  event_sequence INTEGER NOT NULL,
  evidence_id BLOB NOT NULL CHECK(length(evidence_id)=32),
  PRIMARY KEY (run_id, event_sequence),
  FOREIGN KEY (run_id, event_sequence) REFERENCES agent_replan_research_checkpoints(run_id, event_sequence)
    ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;
CREATE TRIGGER agent_replan_originals_update_guard BEFORE UPDATE ON agent_replan_originals
BEGIN SELECT RAISE(ABORT, 'replan originals are immutable'); END;
CREATE TRIGGER agent_replan_originals_delete_guard BEFORE DELETE ON agent_replan_originals
BEGIN SELECT RAISE(ABORT, 'replan originals are append-only'); END;
