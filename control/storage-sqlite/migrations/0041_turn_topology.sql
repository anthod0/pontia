ALTER TABLE events ADD COLUMN turn_topology TEXT;

ALTER TABLE turns ADD COLUMN parent_turn_id TEXT;
ALTER TABLE turns ADD COLUMN topology_status TEXT NOT NULL DEFAULT 'unknown';

CREATE TRIGGER turns_validate_topology_on_insert
BEFORE INSERT ON turns
WHEN NOT (
    (NEW.topology_status IN ('unknown', 'root') AND NEW.parent_turn_id IS NULL)
    OR (NEW.topology_status = 'linked' AND NEW.parent_turn_id IS NOT NULL AND trim(NEW.parent_turn_id) <> '')
)
BEGIN
    SELECT RAISE(ABORT, 'invalid Turn topology status/parent combination');
END;

CREATE TRIGGER turns_validate_topology_on_update
BEFORE UPDATE OF topology_status, parent_turn_id ON turns
WHEN NOT (
    (NEW.topology_status IN ('unknown', 'root') AND NEW.parent_turn_id IS NULL)
    OR (NEW.topology_status = 'linked' AND NEW.parent_turn_id IS NOT NULL AND trim(NEW.parent_turn_id) <> '')
)
BEGIN
    SELECT RAISE(ABORT, 'invalid Turn topology status/parent combination');
END;

CREATE TRIGGER turns_validate_linked_parent_on_insert
BEFORE INSERT ON turns
WHEN NEW.topology_status = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = NEW.parent_turn_id
      AND parent.session_id = NEW.session_id
      AND parent.turn_index < NEW.turn_index
 )
BEGIN
    SELECT RAISE(ABORT, 'linked parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER turns_validate_linked_parent_on_update
BEFORE UPDATE OF topology_status, parent_turn_id, session_id, turn_index ON turns
WHEN NEW.topology_status = 'linked'
 AND NOT EXISTS (
    SELECT 1
    FROM turns AS parent
    WHERE parent.turn_id = NEW.parent_turn_id
      AND parent.session_id = NEW.session_id
      AND parent.turn_index < NEW.turn_index
 )
BEGIN
    SELECT RAISE(ABORT, 'linked parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER turns_preserve_resolved_topology
BEFORE UPDATE OF topology_status, parent_turn_id ON turns
WHEN OLD.topology_status <> 'unknown'
 AND (NEW.topology_status IS NOT OLD.topology_status OR NEW.parent_turn_id IS NOT OLD.parent_turn_id)
BEGIN
    SELECT RAISE(ABORT, 'resolved Turn topology is immutable');
END;

CREATE TRIGGER turn_events_validate_topology
BEFORE INSERT ON events
WHEN NEW.turn_topology IS NOT NULL
 AND (
    NEW.event_type <> 'turn.started'
    OR NOT json_valid(NEW.turn_topology)
    OR COALESCE(json_extract(NEW.turn_topology, '$.status'), '') NOT IN ('unknown', 'root', 'linked')
    OR (
        json_extract(NEW.turn_topology, '$.status') IN ('unknown', 'root')
        AND json_type(NEW.turn_topology, '$.parent_turn_id') IS NOT NULL
    )
    OR (
        json_extract(NEW.turn_topology, '$.status') = 'linked'
        AND COALESCE(trim(json_extract(NEW.turn_topology, '$.parent_turn_id')), '') = ''
    )
 )
BEGIN
    SELECT RAISE(ABORT, 'invalid turn.started topology enrichment');
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
      AND parent.turn_index < NEW.turn_index
 )
BEGIN
    SELECT RAISE(ABORT, 'linked event parent must be an earlier Turn in the same Session');
END;

CREATE TRIGGER events_preserve_turn_topology
BEFORE UPDATE OF turn_topology ON events
WHEN NEW.turn_topology IS NOT OLD.turn_topology
BEGIN
    SELECT RAISE(ABORT, 'event Turn topology enrichment is immutable');
END;
