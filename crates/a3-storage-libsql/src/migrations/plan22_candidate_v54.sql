CREATE TABLE source_caller_relocalizations (
    acquisition_id BLOB PRIMARY KEY NOT NULL CHECK (length(acquisition_id) = 32),
    scope_digest BLOB NOT NULL CHECK (length(scope_digest) = 32),
    policy_version INTEGER NOT NULL CHECK (policy_version = 1),
    run_id BLOB NOT NULL CHECK (length(run_id) = 32),
    step_id BLOB NOT NULL CHECK (length(step_id) = 32),
    attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 4294967295),
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 8),
    tool_run_id BLOB NOT NULL UNIQUE CHECK (length(tool_run_id) = 32),
    tool_attempt INTEGER NOT NULL CHECK (tool_attempt BETWEEN 1 AND 4294967295),
    after_sequence INTEGER NOT NULL CHECK (after_sequence >= 1),
    callee_id BLOB NOT NULL CHECK (length(callee_id) = 32),
    observation TEXT CHECK (observation IN ('no_match','candidate_metadata','incomplete')),
    UNIQUE (run_id, step_id, attempt_number, ordinal),
    UNIQUE (run_id, step_id, attempt_number, callee_id),
    FOREIGN KEY (run_id, after_sequence) REFERENCES run_events(run_id, event_sequence),
    FOREIGN KEY (tool_run_id, tool_attempt) REFERENCES tool_run_attempts(tool_run_id, attempt_sequence)
) STRICT;

CREATE TRIGGER source_caller_insert_unobserved
BEFORE INSERT ON source_caller_relocalizations
WHEN NEW.observation IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'caller relocalization admission must precede observation');
END;

CREATE TRIGGER source_caller_observe_once
BEFORE UPDATE ON source_caller_relocalizations
WHEN OLD.observation IS NOT NULL OR NEW.observation IS NULL
    OR NEW.acquisition_id != OLD.acquisition_id OR NEW.scope_digest != OLD.scope_digest
    OR NEW.policy_version != OLD.policy_version OR NEW.run_id != OLD.run_id
    OR NEW.step_id != OLD.step_id OR NEW.attempt_number != OLD.attempt_number
    OR NEW.ordinal != OLD.ordinal OR NEW.tool_run_id != OLD.tool_run_id
    OR NEW.tool_attempt != OLD.tool_attempt OR NEW.after_sequence != OLD.after_sequence
    OR NEW.callee_id != OLD.callee_id
BEGIN
    SELECT RAISE(ABORT, 'caller relocalization receipt is immutable');
END;

CREATE TRIGGER source_caller_no_delete
BEFORE DELETE ON source_caller_relocalizations
BEGIN
    SELECT RAISE(ABORT, 'caller relocalization cannot be refunded');
END;
