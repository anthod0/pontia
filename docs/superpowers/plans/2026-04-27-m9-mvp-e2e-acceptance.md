# M9 MVP End-to-End Acceptance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the backend-only Control Plane MVP can be driven end-to-end through HTTP APIs with stable errors, idempotency, projections, artifact reads, and documentation.

**Architecture:** Add a focused M9 integration test that exercises the External API, Internal Event API, generic adapter contract, SQLite-backed projections, artifact index/content reads, lifecycle controls, and error mapping through the existing Axum router. Only fill implementation gaps discovered by the test; do not add Web UI, WebSocket, approval, or client-specific adapters.

**Tech Stack:** Rust, Tokio, Axum test router, SQLx/SQLite, Serde JSON, tempfile, existing integration-test helpers.

---

## File Structure

- Create `tests/milestone9_mvp_e2e.rs`: end-to-end orchestration, error semantics, idempotency, and post-MVP exclusion checks.
- Modify `src/transport/http/external.rs`: adjust capability-unavailable status if needed by tests.
- Modify `README.md`: document MVP status and runnable HTTP examples for External API, Internal Event API, and generic adapter contract.
- Modify `MILESTONE.md`: mark M9 complete after verification.

## Task 1: End-to-end orchestration test

- [ ] Write a failing integration test that creates a session, submits a turn, posts adapter events through Internal Event API, registers an artifact, reads turn/events/artifacts/content, interrupts unsupported generic runtime, and terminates the session.
- [ ] Run `cargo test --test milestone9_mvp_e2e orchestrator_can_complete_backend_only_http_polling_flow -- --nocapture` and confirm RED or targeted compile failures for missing test support.
- [ ] Implement only missing support needed by the test.
- [ ] Re-run the test and confirm GREEN.

## Task 2: Error semantics and idempotency tests

- [ ] Add tests for 401 auth failure, 400 invalid request, 404 not found, 409 state conflict, 422 capability unavailable, and idempotent session/turn creation retries.
- [ ] Run `cargo test --test milestone9_mvp_e2e -- --nocapture`; confirm failures identify real gaps.
- [ ] Patch minimal code, expected primarily External API error status mapping.
- [ ] Re-run M9 tests and confirm GREEN.

## Task 3: Documentation and milestone completion

- [ ] Update README with M9 status, MVP end-to-end flow, Internal Event API, artifact, control, and generic adapter contract notes.
- [ ] Mark M9 complete in `MILESTONE.md` with verification commands.
- [ ] Run `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features -- -D warnings`.
