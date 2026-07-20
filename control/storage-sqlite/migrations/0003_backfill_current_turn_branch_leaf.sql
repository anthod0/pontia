-- current_turn_id is the latest reliably observed current branch leaf.
-- Pontia-owned creation/queueing intent is not branch-selection evidence.
UPDATE sessions
SET current_turn_id = (
    SELECT events.turn_id
    FROM events
    WHERE events.session_id = sessions.session_id
      AND events.event_type = 'turn.started'
      AND events.turn_id IS NOT NULL
      AND events.turn_index IS NOT NULL
    ORDER BY events.turn_index DESC, events.rowid DESC, events.event_id DESC
    LIMIT 1
);
