-- Materialized, versioned research checkpoints, atomically owned by the trace event.
-- Contains public outcomes and original references, never retained source windows or raw replies.
CREATE TABLE agent_research_work_checkpoints (
  worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
  session_id BLOB NOT NULL CHECK (length(session_id) = 32),
  user_sequence INTEGER NOT NULL CHECK (user_sequence > 0),
  event_sequence INTEGER NOT NULL CHECK (event_sequence BETWEEN 1 AND 1024),
  work_revision INTEGER NOT NULL CHECK (work_revision BETWEEN 1 AND 65536),
  schema_version INTEGER NOT NULL CHECK (schema_version = 1),
  payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) BETWEEN 1 AND 524288),
  PRIMARY KEY (worktree_id, session_id, user_sequence, event_sequence),
  FOREIGN KEY (worktree_id, session_id, user_sequence, event_sequence)
    REFERENCES agent_work_trace_events(worktree_id, session_id, user_sequence, event_sequence)
    ON UPDATE RESTRICT ON DELETE CASCADE
) STRICT;

CREATE TRIGGER agent_research_work_update_guard BEFORE UPDATE ON agent_research_work_checkpoints
BEGIN SELECT RAISE(ABORT, 'research checkpoints are immutable'); END;
CREATE TRIGGER agent_research_work_delete_guard BEFORE DELETE ON agent_research_work_checkpoints
WHEN EXISTS (
  SELECT 1 FROM agent_work_trace_events
  WHERE worktree_id = OLD.worktree_id AND session_id = OLD.session_id
    AND user_sequence = OLD.user_sequence AND event_sequence = OLD.event_sequence
)
BEGIN SELECT RAISE(ABORT, 'research checkpoints are append-only'); END;
