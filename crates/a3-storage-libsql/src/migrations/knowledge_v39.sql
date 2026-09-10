-- Durable, bounded Fast-Index inspector journal. It deliberately has no foreign key to
-- regenerable index_runs or snapshots, so a rebuild cannot erase diagnostic evidence.
CREATE TABLE index_trace_runs (
  worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
  trace_id BLOB NOT NULL CHECK (length(trace_id) = 32),
  revision INTEGER NOT NULL CHECK (revision > 0),
  state TEXT NOT NULL CHECK (state IN
    ('queued', 'running', 'cancelling', 'succeeded', 'failed', 'cancelled', 'interrupted')),
  trigger_kind TEXT NOT NULL CHECK (trigger_kind IN
    ('initial_observation', 'file_changes', 'recovery_rescan', 'manual_retry')),
  started_at_unix_millis INTEGER NOT NULL CHECK (started_at_unix_millis >= 0),
  ended_at_unix_millis INTEGER CHECK (ended_at_unix_millis IS NULL OR ended_at_unix_millis >= started_at_unix_millis),
  last_activity_at_unix_millis INTEGER NOT NULL CHECK (last_activity_at_unix_millis >= started_at_unix_millis),
  current_phase TEXT CHECK (current_phase IS NULL OR current_phase IN
    ('discover', 'hash', 'parse', 'link', 'rank', 'publish')),
  current_path BLOB CHECK (current_path IS NULL OR length(current_path) BETWEEN 1 AND 131072),
  diagnostic_code TEXT CHECK (diagnostic_code IS NULL OR diagnostic_code IN
    ('discovery', 'source_unavailable', 'revision_changed', 'parse', 'link', 'rank',
     'publish', 'resource_limit', 'timeout', 'progress_unavailable', 'journal_incomplete',
     'worker_unavailable', 'interrupted')),
  details_incomplete INTEGER NOT NULL CHECK (details_incomplete IN (0, 1)),
  previous_publication_available INTEGER NOT NULL CHECK (previous_publication_available IN (0, 1)),
  discovered_count INTEGER NOT NULL CHECK (discovered_count >= 0),
  pending_count INTEGER NOT NULL CHECK (pending_count >= 0),
  new_count INTEGER NOT NULL CHECK (new_count >= 0),
  changed_count INTEGER NOT NULL CHECK (changed_count >= 0),
  unchanged_count INTEGER NOT NULL CHECK (unchanged_count >= 0),
  deleted_count INTEGER NOT NULL CHECK (deleted_count >= 0),
  hashed_count INTEGER NOT NULL CHECK (hashed_count >= 0),
  hash_reused_count INTEGER NOT NULL CHECK (hash_reused_count >= 0),
  structural_count INTEGER NOT NULL CHECK (structural_count >= 0),
  parse_reused_count INTEGER NOT NULL CHECK (parse_reused_count >= 0),
  generic_count INTEGER NOT NULL CHECK (generic_count >= 0),
  failed_count INTEGER NOT NULL CHECK (failed_count >= 0),
  PRIMARY KEY (worktree_id, trace_id),
  CHECK ((state IN ('succeeded', 'failed', 'cancelled', 'interrupted')) = (ended_at_unix_millis IS NOT NULL)),
  CHECK (pending_count + new_count + changed_count + unchanged_count = discovered_count),
  CHECK (pending_count + hashed_count + hash_reused_count = discovered_count),
  CHECK (structural_count + parse_reused_count + generic_count + failed_count <= discovered_count)
) STRICT;

CREATE INDEX index_trace_runs_retention_idx
  ON index_trace_runs (worktree_id, started_at_unix_millis DESC);

CREATE TABLE index_trace_phases (
  worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
  trace_id BLOB NOT NULL CHECK (length(trace_id) = 32),
  phase_ordinal INTEGER NOT NULL CHECK (phase_ordinal BETWEEN 0 AND 5),
  phase TEXT NOT NULL CHECK (phase IN ('discover', 'hash', 'parse', 'link', 'rank', 'publish')),
  state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'succeeded', 'failed', 'cancelled')),
  started_at_unix_millis INTEGER CHECK (started_at_unix_millis IS NULL OR started_at_unix_millis >= 0),
  ended_at_unix_millis INTEGER CHECK (ended_at_unix_millis IS NULL OR ended_at_unix_millis >= 0),
  completed_count INTEGER CHECK (completed_count IS NULL OR completed_count >= 0),
  total_count INTEGER CHECK (total_count IS NULL OR total_count >= 0),
  PRIMARY KEY (worktree_id, trace_id, phase_ordinal),
  FOREIGN KEY (worktree_id, trace_id) REFERENCES index_trace_runs(worktree_id, trace_id)
    ON UPDATE RESTRICT ON DELETE CASCADE,
  CHECK ((completed_count IS NULL) = (total_count IS NULL)),
  CHECK (completed_count IS NULL OR completed_count <= total_count),
  CHECK (ended_at_unix_millis IS NULL OR started_at_unix_millis IS NOT NULL),
  CHECK (ended_at_unix_millis IS NULL OR ended_at_unix_millis >= started_at_unix_millis)
) STRICT;

CREATE TABLE index_trace_events (
  worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
  trace_id BLOB NOT NULL CHECK (length(trace_id) = 32),
  revision INTEGER NOT NULL CHECK (revision > 0),
  occurred_at_unix_millis INTEGER NOT NULL CHECK (occurred_at_unix_millis >= 0),
  event_kind TEXT NOT NULL CHECK (event_kind IN
    ('queued', 'started', 'phase_started', 'progress', 'file_observed', 'diagnostic',
     'cancellation_requested', 'succeeded', 'failed', 'cancelled', 'interrupted')),
  phase TEXT CHECK (phase IS NULL OR phase IN ('discover', 'hash', 'parse', 'link', 'rank', 'publish')),
  repository_path BLOB CHECK (repository_path IS NULL OR length(repository_path) BETWEEN 1 AND 131072),
  diagnostic_code TEXT CHECK (diagnostic_code IS NULL OR diagnostic_code IN
    ('discovery', 'source_unavailable', 'revision_changed', 'parse', 'link', 'rank',
     'publish', 'resource_limit', 'timeout', 'progress_unavailable', 'journal_incomplete',
     'worker_unavailable', 'interrupted')),
  PRIMARY KEY (worktree_id, trace_id, revision),
  FOREIGN KEY (worktree_id, trace_id) REFERENCES index_trace_runs(worktree_id, trace_id)
    ON UPDATE RESTRICT ON DELETE CASCADE
) STRICT;

CREATE TABLE index_trace_files (
  worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),
  trace_id BLOB NOT NULL CHECK (length(trace_id) = 32),
  repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),
  path_search TEXT NOT NULL CHECK (length(CAST(path_search AS BLOB)) BETWEEN 1 AND 2048),
  change_kind TEXT NOT NULL CHECK (change_kind IN ('pending', 'new', 'changed', 'unchanged', 'deleted')),
  hash_outcome TEXT NOT NULL CHECK (hash_outcome IN ('pending', 'hashed', 'reused', 'not_applicable')),
  parse_outcome TEXT CHECK (parse_outcome IS NULL OR parse_outcome IN
    ('structural', 'reused', 'generic', 'failed', 'not_applicable')),
  diagnostic_codes TEXT NOT NULL CHECK (length(CAST(diagnostic_codes AS BLOB)) <= 512),
  diagnostics_truncated INTEGER NOT NULL CHECK (diagnostics_truncated IN (0, 1)),
  PRIMARY KEY (worktree_id, trace_id, repository_path),
  FOREIGN KEY (worktree_id, trace_id) REFERENCES index_trace_runs(worktree_id, trace_id)
    ON UPDATE RESTRICT ON DELETE CASCADE
) STRICT;

CREATE INDEX index_trace_files_search_idx
  ON index_trace_files (worktree_id, trace_id, path_search, repository_path);
