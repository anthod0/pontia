DROP TRIGGER turn_events_validate_topology;

CREATE TRIGGER turn_events_validate_topology
BEFORE INSERT ON events
WHEN (NEW.turn_topology IS NOT NULL AND (
    NEW.event_type NOT IN ('turn.started', 'turn.topology_recovered')
    OR NOT json_valid(NEW.turn_topology)
    OR COALESCE(json_extract(NEW.turn_topology, '$.status'), '') NOT IN ('unknown', 'root', 'linked')
    OR (json_extract(NEW.turn_topology, '$.status') IN ('unknown', 'root')
        AND json_type(NEW.turn_topology, '$.parent_turn_id') IS NOT NULL)
    OR (json_extract(NEW.turn_topology, '$.status') = 'linked'
        AND COALESCE(trim(json_extract(NEW.turn_topology, '$.parent_turn_id')), '') = '')
)) OR (NEW.event_type = 'turn.topology_recovered' AND (
    NEW.source <> 'system_monitor'
    OR NEW.turn_topology IS NULL
    OR COALESCE(json_extract(NEW.turn_topology, '$.status'), '') NOT IN ('root', 'linked')
    OR NOT EXISTS (SELECT 1 FROM turns WHERE turn_id = NEW.turn_id AND session_id = NEW.session_id)
))
BEGIN
    SELECT RAISE(ABORT, 'invalid Turn topology enrichment');
END;
