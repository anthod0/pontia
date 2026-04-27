-- Baseline migration.
-- Domain tables are introduced by later migrations. This migration verifies
-- that SQLx migration wiring is present and leaves SQLite invariants explicit.
PRAGMA foreign_keys = ON;
