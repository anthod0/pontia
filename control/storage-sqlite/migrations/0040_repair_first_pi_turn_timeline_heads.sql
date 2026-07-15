-- Pi does not create a new Session's JSONL file until its first message is
-- persisted. First-Turn head capture can therefore validly start at source
-- origin even when the source is not yet resolvable at turn.started ingestion.
UPDATE events
SET timeline_boundary = (
    SELECT json_object(
        'position', 'head',
        'cursor',
        'pi-jsonl-v2:' || binding.id || ':0:after:' ||
            COALESCE(json_extract(events.payload, '$.timeline_anchor.previous_leaf_id'), '')
    )
    FROM agent_bindings AS binding
    WHERE binding.session_id = events.session_id
      AND binding.client_type = 'pi'
)
WHERE event_type = 'turn.started'
  AND source = 'agent_adapter'
  AND client_type = 'pi'
  AND turn_index = 1
  AND timeline_boundary IS NULL
  AND COALESCE(json_type(payload, '$.timeline_anchor.previous_leaf_id'), 'null') IN ('text', 'null')
  AND EXISTS (
      SELECT 1
      FROM agent_bindings AS binding
      WHERE binding.session_id = events.session_id
        AND binding.client_type = 'pi'
  );

UPDATE turns
SET head_cursor = (
    SELECT json_extract(event.timeline_boundary, '$.cursor')
    FROM events AS event
    WHERE event.session_id = turns.session_id
      AND event.turn_id = turns.turn_id
      AND event.event_type = 'turn.started'
      AND event.source = 'agent_adapter'
      AND event.client_type = 'pi'
      AND event.turn_index = 1
      AND event.timeline_boundary IS NOT NULL
    ORDER BY event.created_at, event.event_id
    LIMIT 1
)
WHERE turn_index = 1
  AND head_cursor IS NULL
  AND EXISTS (
      SELECT 1
      FROM events AS event
      WHERE event.session_id = turns.session_id
        AND event.turn_id = turns.turn_id
        AND event.event_type = 'turn.started'
        AND event.source = 'agent_adapter'
        AND event.client_type = 'pi'
        AND event.turn_index = 1
        AND event.timeline_boundary IS NOT NULL
  );
