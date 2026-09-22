-- Session events may reference a Turn as context without owning a turn_index.
-- The projection-side trigger cannot distinguish that valid association reliably,
-- so remove it while retaining the event-side Turn index constraint.
DROP TRIGGER turns_require_matching_event_turn_index;
