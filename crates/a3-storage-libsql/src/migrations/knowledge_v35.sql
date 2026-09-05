-- V35: row-preserving, atomic replacement. FK enforcement stays enabled.
-- Copy every parent and child before dropping the old child-to-parent chain.
CREATE TABLE agent_work_trace_events_v35 (
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
      session_id BLOB NOT NULL CHECK (length(session_id) = 32),
      user_sequence INTEGER NOT NULL CHECK (user_sequence > 0),
      event_sequence INTEGER NOT NULL CHECK (event_sequence BETWEEN 1 AND 1024),
      phase TEXT NOT NULL CHECK (phase IN
        ('preparing', 'locating', 'deciding', 'reading', 'evaluating', 'answering_or_planning',
         'selecting_evidence', 'searching_source', 'inspecting_source', 'answering', 'completed')),
      state TEXT NOT NULL CHECK (state IN
        ('running', 'completed', 'failed', 'cancelled', 'awaiting_continuation')),
      action TEXT NOT NULL CHECK (length(CAST(action AS BLOB)) BETWEEN 1 AND 512),
      query_text TEXT CHECK (query_text IS NULL OR length(CAST(query_text AS BLOB)) BETWEEN 1 AND 4096),
      completeness TEXT NOT NULL CHECK (completeness IN ('complete', 'limited', 'not_applicable')),
      occurred_at_unix_millis INTEGER NOT NULL CHECK (occurred_at_unix_millis >= 0),
      PRIMARY KEY (worktree_id, session_id, user_sequence, event_sequence),
      FOREIGN KEY (worktree_id, session_id, user_sequence)
        REFERENCES agent_work_trace_turns(worktree_id, session_id, user_sequence)
        ON UPDATE RESTRICT ON DELETE CASCADE
      ) STRICT;
CREATE TABLE agent_work_trace_notes_v35 (
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
      session_id BLOB NOT NULL CHECK (length(session_id) = 32),
      user_sequence INTEGER NOT NULL CHECK (user_sequence > 0),
      event_sequence INTEGER NOT NULL CHECK (event_sequence BETWEEN 1 AND 1024),
      goal TEXT NOT NULL CHECK (length(CAST(goal AS BLOB)) BETWEEN 1 AND 1024),
      finding_kind TEXT NOT NULL CHECK (finding_kind IN ('observation', 'hypothesis', 'conclusion')),
      finding TEXT NOT NULL CHECK (length(CAST(finding AS BLOB)) BETWEEN 1 AND 4096),
      gap TEXT NOT NULL CHECK (length(CAST(gap AS BLOB)) BETWEEN 1 AND 1024),
      next_step TEXT NOT NULL CHECK (length(CAST(next_step AS BLOB)) BETWEEN 1 AND 1024),
      PRIMARY KEY (worktree_id, session_id, user_sequence, event_sequence),
      FOREIGN KEY (worktree_id, session_id, user_sequence, event_sequence)
        REFERENCES agent_work_trace_events_v35(worktree_id, session_id, user_sequence, event_sequence)
        ON UPDATE RESTRICT ON DELETE CASCADE
      ) STRICT;
CREATE TABLE agent_work_trace_note_sources_v35 (
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
      session_id BLOB NOT NULL CHECK (length(session_id) = 32),
      user_sequence INTEGER NOT NULL CHECK (user_sequence > 0),
      event_sequence INTEGER NOT NULL CHECK (event_sequence BETWEEN 1 AND 1024),
      source_position INTEGER NOT NULL CHECK (source_position BETWEEN 1 AND 32),
      source_id BLOB NOT NULL CHECK (length(source_id) = 32),
      PRIMARY KEY (worktree_id, session_id, user_sequence, event_sequence, source_position),
      UNIQUE (worktree_id, session_id, user_sequence, event_sequence, source_id),
      FOREIGN KEY (worktree_id, session_id, user_sequence, event_sequence)
        REFERENCES agent_work_trace_notes_v35(worktree_id, session_id, user_sequence, event_sequence)
        ON UPDATE RESTRICT ON DELETE CASCADE,
      FOREIGN KEY (worktree_id, session_id, user_sequence, source_id)
        REFERENCES agent_work_trace_sources(worktree_id, session_id, user_sequence, source_id)
        ON UPDATE RESTRICT ON DELETE CASCADE
      ) STRICT;
INSERT INTO agent_work_trace_events_v35 SELECT * FROM agent_work_trace_events;
INSERT INTO agent_work_trace_notes_v35 SELECT * FROM agent_work_trace_notes;
INSERT INTO agent_work_trace_note_sources_v35 SELECT * FROM agent_work_trace_note_sources;
DROP TABLE agent_work_trace_note_sources;
DROP TABLE agent_work_trace_notes;
DROP TABLE agent_work_trace_events;
ALTER TABLE agent_work_trace_events_v35 RENAME TO agent_work_trace_events;
CREATE TRIGGER agent_work_trace_events_update_guard BEFORE UPDATE ON agent_work_trace_events BEGIN
  SELECT RAISE(ABORT, 'Agent work trace events are immutable'); END;
ALTER TABLE agent_work_trace_notes_v35 RENAME TO agent_work_trace_notes;
CREATE TRIGGER agent_work_trace_notes_update_guard BEFORE UPDATE ON agent_work_trace_notes BEGIN
  SELECT RAISE(ABORT, 'Agent work trace notes are immutable'); END;
ALTER TABLE agent_work_trace_note_sources_v35 RENAME TO agent_work_trace_note_sources;
CREATE TRIGGER agent_work_trace_note_sources_update_guard BEFORE UPDATE ON agent_work_trace_note_sources BEGIN
  SELECT RAISE(ABORT, 'Agent work trace note sources are immutable'); END;
