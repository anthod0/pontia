-- Turn failure details are sourced from authoritative lifecycle event payloads
-- rather than persisted redundantly in the Turn projection.
ALTER TABLE turns DROP COLUMN failure_message;
