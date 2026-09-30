CREATE TABLE agent_permission_revisions (
    revision INTEGER PRIMARY KEY CHECK (revision > 0),
    mode TEXT NOT NULL CHECK (mode IN ('askPermissions', 'fullMachine'))
) STRICT;
INSERT INTO agent_permission_revisions (revision, mode) VALUES (1, 'askPermissions');
CREATE TRIGGER agent_permission_revisions_update_guard BEFORE UPDATE ON agent_permission_revisions BEGIN
    SELECT RAISE(ABORT, 'Agent permission selections are immutable');
END;
CREATE TRIGGER agent_permission_revisions_delete_guard BEFORE DELETE ON agent_permission_revisions BEGIN
    SELECT RAISE(ABORT, 'Agent permission selections are immutable');
END;
