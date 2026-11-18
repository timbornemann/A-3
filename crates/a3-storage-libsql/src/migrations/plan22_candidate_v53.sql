CREATE TABLE source_read_acquisitions (
    source_read_id BLOB PRIMARY KEY NOT NULL CHECK (length(source_read_id) = 32),
    scope_digest BLOB NOT NULL CHECK (length(scope_digest) = 32),
    policy_version INTEGER NOT NULL CHECK (policy_version = 1),
    run_id BLOB NOT NULL CHECK (length(run_id) = 32),
    step_id BLOB NOT NULL CHECK (length(step_id) = 32),
    attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 4294967295),
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 8),
    tool_run_id BLOB NOT NULL UNIQUE CHECK (length(tool_run_id) = 32),
    tool_attempt INTEGER NOT NULL CHECK (tool_attempt BETWEEN 1 AND 4294967295),
    after_sequence INTEGER NOT NULL CHECK (after_sequence >= 1),
    repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),
    content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),
    required_range BLOB NOT NULL CHECK (length(required_range) = 24),
    original_evidence_id BLOB CHECK (length(original_evidence_id) = 32),
    abandoned INTEGER NOT NULL DEFAULT 0 CHECK (abandoned IN (0, 1)),
    UNIQUE (run_id, step_id, attempt_number, ordinal),
    FOREIGN KEY (run_id, after_sequence) REFERENCES run_events(run_id, event_sequence),
    FOREIGN KEY (tool_run_id, tool_attempt) REFERENCES tool_run_attempts(tool_run_id, attempt_sequence),
    FOREIGN KEY (tool_run_id, original_evidence_id) REFERENCES tool_evidence(tool_run_id, evidence_id)
) STRICT;

CREATE TRIGGER source_read_insert_pending
BEFORE INSERT ON source_read_acquisitions
WHEN NEW.original_evidence_id IS NOT NULL OR NEW.abandoned != 0
BEGIN
    SELECT RAISE(ABORT, 'source read admission must precede completion');
END;

CREATE TRIGGER source_read_identity_immutable
BEFORE UPDATE ON source_read_acquisitions
WHEN NEW.source_read_id != OLD.source_read_id OR NEW.scope_digest != OLD.scope_digest
    OR NEW.policy_version != OLD.policy_version OR NEW.run_id != OLD.run_id
    OR NEW.step_id != OLD.step_id OR NEW.attempt_number != OLD.attempt_number
    OR NEW.ordinal != OLD.ordinal OR NEW.tool_run_id != OLD.tool_run_id
    OR NEW.tool_attempt != OLD.tool_attempt OR NEW.after_sequence != OLD.after_sequence
    OR NEW.repository_path != OLD.repository_path OR NEW.content_hash != OLD.content_hash
    OR NEW.required_range != OLD.required_range
    OR OLD.abandoned = 1
    OR (NEW.original_evidence_id IS NOT OLD.original_evidence_id
        AND (OLD.original_evidence_id IS NOT NULL OR NEW.abandoned = 1))
BEGIN
    SELECT RAISE(ABORT, 'source read acquisition is immutable');
END;

CREATE TRIGGER source_read_no_delete
BEFORE DELETE ON source_read_acquisitions
BEGIN
    SELECT RAISE(ABORT, 'source read acquisition cannot be refunded');
END;

ALTER TABLE edit_phase_reservations
    ADD COLUMN source_read_id BLOB REFERENCES source_read_acquisitions(source_read_id)
    CHECK (length(source_read_id) = 32);

CREATE INDEX edit_phase_reservations_source_read
    ON edit_phase_reservations(source_read_id) WHERE source_read_id IS NOT NULL;

CREATE UNIQUE INDEX edit_phase_one_accepted_per_source_read
    ON edit_phase_reservations(source_read_id)
    WHERE source_read_id IS NOT NULL AND outcome = 'accepted_reply';

CREATE TRIGGER edit_phase_source_read_immutable
BEFORE UPDATE OF source_read_id ON edit_phase_reservations
WHEN NEW.source_read_id IS NOT OLD.source_read_id
BEGIN
    SELECT RAISE(ABORT, 'Edit source read binding is immutable');
END;
