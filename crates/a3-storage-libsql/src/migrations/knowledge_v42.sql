ALTER TABLE run_events ADD COLUMN turn_action_kind_v3 TEXT CHECK (turn_action_kind_v3 IS NULL OR turn_action_kind_v3 = 'machine');
CREATE TRIGGER run_events_machine_action_guard BEFORE INSERT ON run_events
WHEN NEW.turn_action_kind_v3 IS NOT NULL AND (NEW.event_kind <> 'model_interaction' OR NEW.turn_action_kind IS NOT NULL OR NEW.turn_action_kind_v2 IS NOT NULL)
BEGIN SELECT RAISE(ABORT, 'machine turn action is invalid'); END;
DROP TRIGGER mutation_attempts_insert_guard;
DROP TRIGGER mutation_attempts_update_guard;
DROP TRIGGER mutation_attempts_delete_guard;
DROP INDEX mutation_attempts_reconciliation_idx;
ALTER TABLE mutation_attempts RENAME TO mutation_attempts_v22;
CREATE TABLE mutation_attempts (
      tool_run_id BLOB NOT NULL CHECK (length(tool_run_id) = 32),
      attempt_sequence INTEGER NOT NULL CHECK (attempt_sequence BETWEEN 1 AND 4294967295),
      action_fingerprint BLOB NOT NULL CHECK (length(action_fingerprint) = 32),
      action_kind TEXT NOT NULL CHECK (action_kind IN ('patch', 'process',
        'unclassified_legacy', 'machine_file', 'machine_process', 'machine_network')),
      application_state TEXT NOT NULL CHECK (application_state IN
        ('applied', 'not_applied', 'unknown')),
      reconciliation_state TEXT NOT NULL CHECK (reconciliation_state IN
        ('not_required', 'required', 'reconciled', 'replanned')),
      reconciled_snapshot_id BLOB CHECK
        (reconciled_snapshot_id IS NULL OR length(reconciled_snapshot_id) = 32),
      reconciled_at_unix_millis INTEGER CHECK
        (reconciled_at_unix_millis IS NULL OR reconciled_at_unix_millis >= 0),
      CHECK ((application_state IN ('applied', 'not_applied')
          AND reconciliation_state = 'not_required' AND reconciled_snapshot_id IS NULL
          AND reconciled_at_unix_millis IS NULL) OR
        (application_state = 'unknown' AND reconciliation_state = 'required'
          AND reconciled_snapshot_id IS NULL AND reconciled_at_unix_millis IS NULL) OR
        (application_state = 'unknown' AND reconciliation_state IN ('reconciled', 'replanned')
          AND reconciled_snapshot_id IS NOT NULL AND reconciled_at_unix_millis IS NOT NULL)),
      PRIMARY KEY (tool_run_id, attempt_sequence),
      FOREIGN KEY (tool_run_id, attempt_sequence)
        REFERENCES tool_run_attempts(tool_run_id, attempt_sequence)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
      FOREIGN KEY (reconciled_snapshot_id) REFERENCES snapshots(snapshot_id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
      ) STRICT;
INSERT INTO mutation_attempts SELECT * FROM mutation_attempts_v22;
DROP TABLE mutation_attempts_v22;
      CREATE INDEX mutation_attempts_reconciliation_idx
        ON mutation_attempts(application_state, reconciliation_state, tool_run_id,
          attempt_sequence);
      CREATE TRIGGER mutation_attempts_insert_guard
      BEFORE INSERT ON mutation_attempts
      WHEN NOT EXISTS (SELECT 1 FROM tool_run_attempts
        WHERE tool_run_id = NEW.tool_run_id AND attempt_sequence = NEW.attempt_sequence
          AND status = 'in_flight')
      BEGIN SELECT RAISE(ABORT, 'mutation attempt requires an in-flight tool attempt'); END;
      CREATE TRIGGER mutation_attempts_update_guard
      BEFORE UPDATE ON mutation_attempts
      WHEN NEW.tool_run_id <> OLD.tool_run_id
        OR NEW.attempt_sequence <> OLD.attempt_sequence
        OR NEW.action_fingerprint <> OLD.action_fingerprint
        OR NEW.action_kind <> OLD.action_kind
        OR NOT ((OLD.application_state = 'unknown' AND OLD.reconciliation_state = 'required')
          OR (OLD.application_state = 'unknown' AND OLD.reconciliation_state = 'reconciled'
            AND NEW.application_state = 'unknown' AND NEW.reconciliation_state = 'replanned'
            AND NEW.reconciled_snapshot_id = OLD.reconciled_snapshot_id
            AND NEW.reconciled_at_unix_millis = OLD.reconciled_at_unix_millis))
      BEGIN SELECT RAISE(ABORT, 'mutation attempt transition is invalid'); END;
      CREATE TRIGGER mutation_attempts_delete_guard
      BEFORE DELETE ON mutation_attempts
      BEGIN SELECT RAISE(ABORT, 'mutation attempts are append-only'); END;
CREATE TABLE machine_effect_scopes (
    tool_run_id BLOB NOT NULL CHECK (length(tool_run_id) = 32),
    attempt_sequence INTEGER NOT NULL CHECK (attempt_sequence BETWEEN 1 AND 4294967295),
    resource_id BLOB NOT NULL CHECK (length(resource_id) = 32),
    step_id BLOB NOT NULL CHECK (length(step_id) = 32),
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('file', 'process', 'http')),
    target TEXT NOT NULL CHECK (length(CAST(target AS BLOB)) BETWEEN 1 AND 4096),
    expected_hash BLOB CHECK (expected_hash IS NULL OR length(expected_hash) = 32),
    proposed_hash BLOB CHECK (proposed_hash IS NULL OR length(proposed_hash) = 32),
    PRIMARY KEY (tool_run_id, attempt_sequence),
    FOREIGN KEY (tool_run_id, attempt_sequence) REFERENCES mutation_attempts(tool_run_id, attempt_sequence) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;
CREATE TRIGGER machine_effect_scopes_update_guard BEFORE UPDATE ON machine_effect_scopes BEGIN SELECT RAISE(ABORT, 'machine effect scopes are immutable'); END;
CREATE TRIGGER machine_effect_scopes_delete_guard BEFORE DELETE ON machine_effect_scopes BEGIN SELECT RAISE(ABORT, 'machine effect scopes are append-only'); END;

CREATE TABLE machine_recovery_acknowledgements (
    tool_run_id BLOB NOT NULL, attempt_sequence INTEGER NOT NULL,
    action_fingerprint BLOB NOT NULL CHECK (length(action_fingerprint) = 32),
    resource_id BLOB NOT NULL CHECK (length(resource_id) = 32),
    observed_hash BLOB CHECK (observed_hash IS NULL OR length(observed_hash) = 32),
    read_policy_decision_id BLOB CHECK (read_policy_decision_id IS NULL OR length(read_policy_decision_id) = 32),
    acknowledged_at_unix_millis INTEGER NOT NULL CHECK (acknowledged_at_unix_millis >= 0),
    PRIMARY KEY (tool_run_id, attempt_sequence),
    FOREIGN KEY (tool_run_id, attempt_sequence) REFERENCES machine_effect_scopes(tool_run_id, attempt_sequence) ON UPDATE RESTRICT ON DELETE RESTRICT,
    FOREIGN KEY (read_policy_decision_id) REFERENCES policy_decisions(policy_decision_id) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;
CREATE TRIGGER machine_recovery_ack_insert_guard BEFORE INSERT ON machine_recovery_acknowledgements
WHEN NOT EXISTS (SELECT 1 FROM mutation_attempts JOIN machine_effect_scopes USING (tool_run_id, attempt_sequence)
    WHERE tool_run_id = NEW.tool_run_id AND attempt_sequence = NEW.attempt_sequence
    AND action_fingerprint = NEW.action_fingerprint AND resource_id = NEW.resource_id
    AND application_state = 'unknown' AND reconciliation_state = 'required'
    AND ((resource_kind = 'file' AND NEW.read_policy_decision_id IS NOT NULL)
        OR (resource_kind <> 'file' AND NEW.read_policy_decision_id IS NULL AND NEW.observed_hash IS NULL)))
BEGIN SELECT RAISE(ABORT, 'machine recovery requires an exact unknown scope and observation'); END;
CREATE TRIGGER machine_recovery_ack_update_guard BEFORE UPDATE ON machine_recovery_acknowledgements BEGIN SELECT RAISE(ABORT, 'machine recovery acknowledgements are immutable'); END;
CREATE TRIGGER machine_recovery_ack_delete_guard BEFORE DELETE ON machine_recovery_acknowledgements BEGIN SELECT RAISE(ABORT, 'machine recovery acknowledgements are append-only'); END;
