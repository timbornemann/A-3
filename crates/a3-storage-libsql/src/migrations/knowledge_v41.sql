CREATE TABLE policy_permission_context (
    policy_decision_id BLOB PRIMARY KEY NOT NULL CHECK (length(policy_decision_id) = 32),
    permission_revision INTEGER NOT NULL CHECK (permission_revision > 0),
    permission_mode TEXT NOT NULL CHECK (permission_mode IN ('askPermissions', 'fullMachine')),
    FOREIGN KEY (policy_decision_id) REFERENCES policy_decisions(policy_decision_id)
        ON UPDATE RESTRICT ON DELETE CASCADE
) STRICT;
CREATE TRIGGER policy_permission_context_update_guard BEFORE UPDATE ON policy_permission_context BEGIN
    SELECT RAISE(ABORT, 'Policy permission context is immutable');
END;
