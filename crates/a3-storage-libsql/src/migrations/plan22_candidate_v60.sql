-- Failed responses are diagnostic attempt data, never VerificationEvidence.
ALTER TABLE tool_run_attempts ADD COLUMN has_loopback_mismatch INTEGER NOT NULL DEFAULT 0
    CHECK (has_loopback_mismatch IN (0,1) AND (has_loopback_mismatch=0 OR status='failed'));
CREATE TABLE loopback_mismatch_diagnostics (
    tool_run_id BLOB NOT NULL CHECK (length(tool_run_id)=32),
    attempt_sequence INTEGER NOT NULL CHECK (attempt_sequence BETWEEN 1 AND 4294967295),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id)=32),
    digest BLOB NOT NULL CHECK (length(digest)=32),
    diagnostic BLOB NOT NULL CHECK (length(diagnostic) BETWEEN 400 AND 133000),
    PRIMARY KEY(tool_run_id,attempt_sequence),
    FOREIGN KEY(tool_run_id,attempt_sequence) REFERENCES tool_run_attempts(tool_run_id,attempt_sequence) ON DELETE RESTRICT,
    FOREIGN KEY(event_id) REFERENCES run_events(event_id) ON DELETE RESTRICT
) STRICT;
CREATE TRIGGER loopback_mismatch_requires_marker BEFORE INSERT ON loopback_mismatch_diagnostics
WHEN NOT EXISTS(SELECT 1 FROM tool_run_attempts WHERE tool_run_id=NEW.tool_run_id AND attempt_sequence=NEW.attempt_sequence AND has_loopback_mismatch=1 AND status='failed')
BEGIN SELECT RAISE(ABORT,'Mismatch requires failed marked attempt'); END;
CREATE TRIGGER loopback_mismatch_immutable BEFORE UPDATE ON loopback_mismatch_diagnostics
BEGIN SELECT RAISE(ABORT,'Mismatch is immutable'); END;
CREATE TRIGGER loopback_mismatch_no_delete BEFORE DELETE ON loopback_mismatch_diagnostics
BEGIN SELECT RAISE(ABORT,'Mismatch cannot be removed'); END;
