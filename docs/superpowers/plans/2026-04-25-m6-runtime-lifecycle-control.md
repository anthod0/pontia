# M6 Runtime Lifecycle Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add External API runtime lifecycle controls for interrupt, terminate, and restart with event-driven projection updates and idempotency.

**Architecture:** Follow existing M4/M5 command-service pattern in `src/application/mod.rs`. HTTP handlers stay in `src/transport/http/external.rs`; routes stay in `src/transport/http/mod.rs`; runtime capability decisions stay behind `src/runtime/mod.rs`. Generic runtime keeps `interrupt: false`, so interrupt requests return `capability_unavailable` without faking success.

**Tech Stack:** Rust, Axum, SQLx/SQLite, Tokio integration tests, serde_json.

---

### Task 1: Add failing M6 HTTP tests

**Files:**
- Create: `tests/milestone6_runtime_lifecycle.rs`

- [ ] Write tests for generic interrupt returning `capability_unavailable` for current and specified turn.
- [ ] Write tests for terminate producing terminal session state and idempotent replay.
- [ ] Write tests for restart on non-terminal session producing a new starting/ready lifecycle and rejecting terminal sessions.
- [ ] Run `cargo test --test milestone6_runtime_lifecycle` and verify failures because routes/handlers are missing.

### Task 2: Add runtime lifecycle application service

**Files:**
- Modify: `src/application/mod.rs`
- Modify: `src/runtime/mod.rs`
- Modify: `src/error.rs`

- [ ] Add `CapabilityUnavailable` error mapped by HTTP layer.
- [ ] Add `RuntimeControlService` with `interrupt_current_turn`, `interrupt_turn`, `terminate_session`, and `restart_session`.
- [ ] Reuse existing event ingest and idempotency-key table.
- [ ] For generic interrupt, inspect session capabilities and return `CapabilityUnavailable` before emitting events.
- [ ] For terminate, emit `session.exited`; repeated DELETE on terminal session returns current session.
- [ ] For restart, reject terminal sessions; otherwise emit `session.starting`, upsert runtime binding, emit `session.started` and `session.ready`.
- [ ] Run M6 tests and verify application behavior.

### Task 3: Expose External API routes and handlers

**Files:**
- Modify: `src/transport/http/external.rs`
- Modify: `src/transport/http/mod.rs`

- [ ] Add handlers for session interrupt, turn interrupt, delete session, and restart.
- [ ] Map `CapabilityUnavailable` to HTTP 409 with code `capability_unavailable`.
- [ ] Wire POST/DELETE routes.
- [ ] Run `cargo test --test milestone6_runtime_lifecycle` and verify pass.

### Task 4: Update docs and milestone status

**Files:**
- Modify: `README.md`
- Modify: `MILESTONE.md`

- [ ] Update README current implementation summary and examples if needed.
- [ ] Mark M6 complete with summary and validation commands.
- [ ] Run full verification: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`.
