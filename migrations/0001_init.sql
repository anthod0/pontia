-- Milestone 0 baseline migration.
-- Domain tables are introduced in later milestones. This migration verifies
-- that SQLx migration wiring is present and leaves SQLite invariants explicit.
PRAGMA foreign_keys = ON;
