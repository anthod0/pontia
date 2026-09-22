-- Turn identity and ordering are defined by UUIDv7 turn_id. Remove the legacy
-- session-local ordinal and preserve the surviving identity/topology invariants.
DROP INDEX IF EXISTS idx_turns_session_turn_index;

DROP TRIGGER IF EXISTS turns_require_turn_index;
DROP TRIGGER IF EXISTS turns_require_matching_event_turn_index;
DROP TRIGGER IF EXISTS turns_preserve_turn_identity;
DROP TRIGGER IF EXISTS turn_events_require_turn_identity;
DROP TRIGGER IF EXISTS turn_events_require_matching_turn_index;
DROP TRIGGER IF EXISTS session_events_reject_turn_index;
DROP TRIGGER IF EXISTS events_preserve_turn_index;
DROP TRIGGER IF EXISTS turns_validate_linked_parent_on_insert;
DROP TRIGGER IF EXISTS turns_validate_linked_parent_on_update;
DROP TRIGGER IF EXISTS turn_events_validate_linked_parent;

ALTER TABLE events DROP COLUMN turn_index;
ALTER TABLE turns DROP COLUMN turn_index;
ALTER TABLE sessions DROP COLUMN next_turn_index;

CREATE TRIGGER turns_preserve_turn_identity
BEFORE UPDATE OF session_id ON turns
WHEN NEW.session_id IS NOT OLD.session_id
BEGIN
    SELECT RAISE(ABORT, 'turn session_id is immutable');
END;

CREATE TRIGGER turn_events_require_turn_identity
BEFORE INSERT ON events
WHEN NEW.event_type LIKE 'turn.%'
 AND NEW.turn_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'turn event turn_id is required');
END;

CREATE TRIGGER turns_validate_linked_parent_on_insert
BEFORE INSERT ON turns
WHEN NEW.topology_status = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = NEW.parent_turn_id
      AND parent.session_id = NEW.session_id
      AND parent.turn_id < NEW.turn_id
 )
BEGIN
    SELECT RAISE(ABORT, 'linked parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER turns_validate_linked_parent_on_update
BEFORE UPDATE OF topology_status, parent_turn_id, session_id ON turns
WHEN NEW.topology_status = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = NEW.parent_turn_id
      AND parent.session_id = NEW.session_id
      AND parent.turn_id < NEW.turn_id
 )
BEGIN
    SELECT RAISE(ABORT, 'linked parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER turn_events_validate_linked_parent
BEFORE INSERT ON events
WHEN NEW.turn_topology IS NOT NULL
 AND json_valid(NEW.turn_topology)
 AND json_extract(NEW.turn_topology, '$.status') = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = json_extract(NEW.turn_topology, '$.parent_turn_id')
      AND parent.session_id = NEW.session_id
      AND parent.turn_id < NEW.turn_id
 )
BEGIN
    SELECT RAISE(ABORT, 'linked event parent must be an earlier Turn in the same Session');
END;
