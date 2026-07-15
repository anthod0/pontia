CREATE TABLE migration_0037_agent_binding_guard (valid INTEGER NOT NULL);

CREATE TRIGGER migration_0037_session_binding_guard_failure
BEFORE INSERT ON migration_0037_agent_binding_guard
WHEN NEW.valid = 0
BEGIN
    SELECT RAISE(ABORT, 'migration 0037 found multiple Agent bindings for one Session');
END;

INSERT INTO migration_0037_agent_binding_guard(valid)
SELECT NOT EXISTS (
    SELECT session_id
    FROM agent_bindings
    GROUP BY session_id
    HAVING COUNT(*) > 1
);

DROP TRIGGER migration_0037_session_binding_guard_failure;

CREATE TRIGGER migration_0037_client_identity_guard_failure
BEFORE INSERT ON migration_0037_agent_binding_guard
WHEN NEW.valid = 0
BEGIN
    SELECT RAISE(ABORT, 'migration 0037 found one client identity bound to multiple Sessions');
END;

INSERT INTO migration_0037_agent_binding_guard(valid)
SELECT NOT EXISTS (
    SELECT client_type, client_session_key
    FROM agent_bindings
    GROUP BY client_type, client_session_key
    HAVING COUNT(*) > 1
);

DROP TRIGGER migration_0037_client_identity_guard_failure;
DROP TABLE migration_0037_agent_binding_guard;

CREATE UNIQUE INDEX idx_agent_bindings_one_per_session
ON agent_bindings(session_id);

CREATE UNIQUE INDEX idx_agent_bindings_unique_client_identity
ON agent_bindings(client_type, client_session_key);
