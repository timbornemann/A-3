-- Fresh isolated candidate only. No old development data is adopted or promoted.
ALTER TABLE edit_phase_reservations ADD COLUMN terminal_reply_kind TEXT
    CHECK(terminal_reply_kind IS NULL OR terminal_reply_kind='no_change');
CREATE TRIGGER edit_no_change_tag_insert_null BEFORE INSERT ON edit_phase_reservations
WHEN NEW.terminal_reply_kind IS NOT NULL
BEGIN SELECT RAISE(ABORT,'NoChange tag requires terminal observation'); END;
CREATE TRIGGER edit_no_change_tag_finish_once BEFORE UPDATE OF terminal_reply_kind ON edit_phase_reservations
WHEN OLD.terminal_reply_kind IS NOT NULL OR NEW.terminal_reply_kind IS NULL
  OR OLD.outcome IS NOT NULL OR NEW.outcome IS NOT 'rejected_reply'
BEGIN SELECT RAISE(ABORT,'NoChange tag is part of one closed finish'); END;
CREATE TABLE edit_phase_no_change_receipts (
    run_id BLOB NOT NULL CHECK(length(run_id)=32),
    job_digest BLOB NOT NULL CHECK(length(job_digest)=32),
    call_kind TEXT NOT NULL CHECK(call_kind IN ('initial','structural_repair')),
    record_version INTEGER NOT NULL CHECK(record_version=1),
    index_run_id BLOB NOT NULL CHECK(length(index_run_id)=32),
    profile_id BLOB NOT NULL CHECK(length(profile_id)=32),
    profile_version INTEGER NOT NULL CHECK(profile_version BETWEEN 1 AND 4294967295),
    generation_limit INTEGER NOT NULL CHECK(generation_limit BETWEEN 1 AND 4294967295),
    context_limit INTEGER NOT NULL CHECK(context_limit BETWEEN 1 AND 4294967295),
    original_goal_digest BLOB NOT NULL CHECK(length(original_goal_digest)=32),
    repository_path BLOB NOT NULL CHECK(length(repository_path) BETWEEN 1 AND 131072),
    content_hash BLOB NOT NULL CHECK(length(content_hash)=32),
    window_start INTEGER NOT NULL CHECK(window_start BETWEEN 0 AND 4294967295),
    window_end INTEGER NOT NULL CHECK(window_end>window_start AND window_end-window_start<=8192),
    window_digest BLOB NOT NULL CHECK(length(window_digest)=32),
    source_read_id BLOB NOT NULL CHECK(length(source_read_id)=32),
    original_range BLOB NOT NULL CHECK(length(original_range)=24),
    PRIMARY KEY(run_id,job_digest,call_kind),
    FOREIGN KEY(run_id,job_digest,call_kind)
      REFERENCES edit_phase_reservations(run_id,job_digest,call_kind),
    FOREIGN KEY(source_read_id) REFERENCES source_read_acquisitions(source_read_id)
) STRICT;

CREATE TRIGGER edit_no_change_only_after_known_finish BEFORE INSERT ON edit_phase_no_change_receipts
WHEN NOT EXISTS(SELECT 1 FROM edit_phase_reservations e
    WHERE e.run_id=NEW.run_id AND e.job_digest=NEW.job_digest AND e.call_kind=NEW.call_kind
      AND e.outcome='rejected_reply' AND e.terminal_reply_kind='no_change'
      AND e.source_read_id=NEW.source_read_id
      AND e.finished_at IS NOT NULL AND e.reported_prompt>0 AND e.reported_output>0)
BEGIN SELECT RAISE(ABORT,'NoChange requires its known charged terminal reply'); END;
CREATE TRIGGER edit_no_change_no_update BEFORE UPDATE ON edit_phase_no_change_receipts
BEGIN SELECT RAISE(ABORT,'NoChange receipt is immutable'); END;
CREATE TRIGGER edit_no_change_no_delete BEFORE DELETE ON edit_phase_no_change_receipts
BEGIN SELECT RAISE(ABORT,'NoChange receipt cannot be refunded'); END;
