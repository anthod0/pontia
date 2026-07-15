CREATE TABLE migration_0036_turn_order_guard (valid INTEGER NOT NULL);

CREATE TRIGGER migration_0036_turn_order_guard_failure
BEFORE INSERT ON migration_0036_turn_order_guard
WHEN NEW.valid = 0
BEGIN
    SELECT RAISE(ABORT, 'migration 0036 cannot reconcile historical Turn projections and Turn events');
END;

INSERT INTO migration_0036_turn_order_guard(valid)
SELECT NOT EXISTS (
    SELECT 1
    FROM turns t
    LEFT JOIN events e
      ON e.session_id = t.session_id
     AND e.turn_id = t.turn_id
     AND e.event_type LIKE 'turn.%'
    WHERE e.event_id IS NULL
)
AND NOT EXISTS (
    SELECT 1
    FROM events e
    LEFT JOIN turns t
      ON t.session_id = e.session_id
     AND t.turn_id = e.turn_id
    WHERE e.turn_id IS NOT NULL
      AND e.event_type LIKE 'turn.%'
      AND t.turn_id IS NULL
)
AND NOT EXISTS (
    SELECT 1
    FROM events
    WHERE event_type LIKE 'turn.%'
      AND turn_id IS NULL
);

DROP TRIGGER migration_0036_turn_order_guard_failure;
DROP TABLE migration_0036_turn_order_guard;

ALTER TABLE events ADD COLUMN turn_index INTEGER;
ALTER TABLE turns ADD COLUMN turn_index INTEGER;
ALTER TABLE sessions ADD COLUMN next_turn_index INTEGER NOT NULL DEFAULT 1;

WITH ordered_turns AS (
    SELECT
        turn_id,
        ROW_NUMBER() OVER (
            PARTITION BY session_id
            ORDER BY created_at, turn_id
        ) AS allocated_turn_index
    FROM turns
)
UPDATE turns
SET turn_index = (
    SELECT allocated_turn_index
    FROM ordered_turns
    WHERE ordered_turns.turn_id = turns.turn_id
);

UPDATE events
SET turn_index = (
    SELECT turns.turn_index
    FROM turns
    WHERE turns.session_id = events.session_id
      AND turns.turn_id = events.turn_id
)
WHERE event_type LIKE 'turn.%';

UPDATE sessions
SET next_turn_index = COALESCE(
    (SELECT MAX(turns.turn_index) + 1 FROM turns WHERE turns.session_id = sessions.session_id),
    1
);

CREATE UNIQUE INDEX idx_turns_session_turn_index
ON turns(session_id, turn_index);

CREATE TRIGGER turns_require_turn_index
BEFORE INSERT ON turns
WHEN NEW.turn_index IS NULL
BEGIN
    SELECT RAISE(ABORT, 'turn_index is required');
END;

CREATE TRIGGER turns_require_matching_event_turn_index
BEFORE INSERT ON turns
WHEN EXISTS (
    SELECT 1
    FROM events
    WHERE events.session_id = NEW.session_id
      AND events.turn_id = NEW.turn_id
      AND events.turn_index IS NOT NEW.turn_index
)
BEGIN
    SELECT RAISE(ABORT, 'turn projection index must match its event envelope indexes');
END;

CREATE TRIGGER turns_preserve_turn_identity
BEFORE UPDATE OF session_id, turn_index ON turns
WHEN NEW.session_id IS NOT OLD.session_id
  OR NEW.turn_index IS NOT OLD.turn_index
BEGIN
    SELECT RAISE(ABORT, 'turn session_id and turn_index are immutable');
END;

CREATE TRIGGER turn_events_require_turn_identity
BEFORE INSERT ON events
WHEN NEW.event_type LIKE 'turn.%'
 AND (NEW.turn_id IS NULL OR NEW.turn_index IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'turn event turn_id and turn_index are required');
END;

CREATE TRIGGER turn_events_require_matching_turn_index
BEFORE INSERT ON events
WHEN NEW.event_type LIKE 'turn.%'
 AND EXISTS (
    SELECT 1
    FROM turns
    WHERE turns.session_id = NEW.session_id
      AND turns.turn_id = NEW.turn_id
      AND turns.turn_index IS NOT NEW.turn_index
 )
BEGIN
    SELECT RAISE(ABORT, 'turn event index must match the Turn projection index');
END;

CREATE TRIGGER session_events_reject_turn_index
BEFORE INSERT ON events
WHEN NEW.event_type NOT LIKE 'turn.%'
 AND NEW.turn_index IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'session event cannot have turn_index');
END;

CREATE TRIGGER events_preserve_turn_index
BEFORE UPDATE OF turn_index ON events
WHEN NEW.turn_index IS NOT OLD.turn_index
BEGIN
    SELECT RAISE(ABORT, 'event turn_index is immutable');
END;
