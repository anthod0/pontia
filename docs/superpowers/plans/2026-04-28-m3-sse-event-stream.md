# M3 SSE Event Stream Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add authenticated SSE event stream endpoints for session and turn domain events.

**Architecture:** Implement lightweight polling SSE over the existing SQLite event store. External cursors use `event_id`; application queries resolve them to DB row cursors and stream only persisted `EventView` data.

**Tech Stack:** Rust 2024, Axum 0.8, SQLx SQLite, Tokio, SSE (`axum::response::sse`).

---

## File Structure

- Modify `.worktrees/m3-sse/Cargo.toml`: add direct `tokio-stream` dependency if needed by SSE stream construction.
- Modify `.worktrees/m3-sse/src/application/mod.rs`: add `EventStreamItem`, `EventStreamScope`, and cursor-aware query methods on `ExternalQueryService`.
- Modify `.worktrees/m3-sse/src/transport/http/external.rs`: add SSE route handlers and query parsing.
- Modify `.worktrees/m3-sse/src/transport/http/mod.rs`: register `/events/stream` routes before non-stream routes if route order matters.
- Create `.worktrees/m3-sse/tests/external_event_stream_api.rs`: integration tests for M3 SSE behavior.
- Modify `.worktrees/m3-sse/README.md`: document stream usage.
- Modify `.worktrees/m3-sse/MILESTONE.md`: mark Milestone 3 complete after tests pass.

## Task 1: Add Failing SSE API Tests

**Files:**
- Create: `.worktrees/m3-sse/tests/external_event_stream_api.rs`

- [ ] **Step 1: Write failing integration tests**

Create tests using `tower::ServiceExt` against `http::router(state)`. Use existing test patterns from `tests/external_api_queries.rs` and read the response body bytes with `http_body_util::BodyExt`.

Test cases:

```rust
#[tokio::test]
async fn event_stream_rejects_missing_or_wrong_bearer_token() { /* expect 401 */ }

#[tokio::test]
async fn session_event_stream_emits_existing_events_as_sse_frames() { /* expect text/event-stream and id/event/data */ }

#[tokio::test]
async fn session_event_stream_after_cursor_resumes_with_later_events_only() { /* seed two events, after first, expect second only */ }

#[tokio::test]
async fn turn_event_stream_only_emits_events_for_requested_turn() { /* seed two turns, expect scoped turn event only */ }

#[tokio::test]
async fn event_stream_rejects_cursor_outside_requested_scope() { /* expect 400 invalid_request */ }
```

Use a header such as `x-llmparty-test-stream-once: true` if needed to make the initial test request terminate after draining currently available events. This header must remain undocumented and test-only in behavior; normal clients should keep streaming.

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cd .worktrees/m3-sse && cargo test --test external_event_stream_api
```

Expected: FAIL because routes/handlers do not exist.

## Task 2: Add Cursor-Aware Event Queries

**Files:**
- Modify: `.worktrees/m3-sse/src/application/mod.rs`

- [ ] **Step 1: Add stream query types and methods**

Add:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct EventStreamItem {
    pub rowid: i64,
    pub event: EventView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStreamScope<'a> {
    Session { session_id: &'a str },
    Turn { session_id: &'a str, turn_id: &'a str },
}
```

Add methods on `ExternalQueryService`:

- `resolve_event_cursor(&self, scope: EventStreamScope<'_>, after_event_id: &str) -> Result<i64>`
- `list_event_stream_items_after(&self, scope: EventStreamScope<'_>, after_rowid: i64, limit: i64) -> Result<Vec<EventStreamItem>>`

Cursor resolution must reject unknown/out-of-scope events with `Error::Domain("event cursor ... is not valid for requested stream")` so External API maps it to `400 invalid_request`.

- [ ] **Step 2: Run RED tests again**

Run:

```bash
cd .worktrees/m3-sse && cargo test --test external_event_stream_api
```

Expected: still FAIL because HTTP routes are missing.

## Task 3: Implement SSE Handlers and Routes

**Files:**
- Modify: `.worktrees/m3-sse/Cargo.toml`
- Modify: `.worktrees/m3-sse/src/transport/http/external.rs`
- Modify: `.worktrees/m3-sse/src/transport/http/mod.rs`

- [ ] **Step 1: Add production code minimally**

In `Cargo.toml`, add direct dependency:

```toml
tokio-stream = "0.1"
```

In `external.rs`, add query struct:

```rust
#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    after: Option<String>,
}
```

Add handlers:

- `stream_session_events(State, HeaderMap, Path<String>, Query<EventStreamQuery>)`
- `stream_turn_events(State, HeaderMap, Path<(String, String)>, Query<EventStreamQuery>)`

Use `axum::response::sse::{Event, KeepAlive, Sse}` and `tokio_stream::wrappers::ReceiverStream`. Build a channel-backed stream that:

1. resolves the cursor;
2. loops querying `list_event_stream_items_after(..., 100)`;
3. sends `Event::default().id(event_id).event("domain_event").json_data(event)`;
4. advances `rowid`;
5. in test-once mode, exits when no immediate rows remain;
6. otherwise sleeps around 200ms before checking again.

Register routes:

```rust
.route("/external/v1/sessions/{session_id}/events/stream", get(external::stream_session_events))
.route("/external/v1/sessions/{session_id}/turns/{turn_id}/events/stream", get(external::stream_turn_events))
```

- [ ] **Step 2: Run SSE tests to verify GREEN**

Run:

```bash
cd .worktrees/m3-sse && cargo test --test external_event_stream_api
```

Expected: PASS.

## Task 4: Regression Tests and Docs

**Files:**
- Modify: `.worktrees/m3-sse/README.md`
- Modify: `.worktrees/m3-sse/MILESTONE.md`

- [ ] **Step 1: Run targeted regression tests**

```bash
cd .worktrees/m3-sse && cargo test --test external_api_queries --test internal_event_api --test mvp_e2e_acceptance --test external_event_stream_api
```

Expected: PASS.

- [ ] **Step 2: Document M3 SSE usage**

Add README section with curl examples for session and turn streams, `after` cursor semantics, and note that polling remains unchanged.

Update `MILESTONE.md` Milestone 3 status and checklist to complete.

- [ ] **Step 3: Run full verification**

```bash
cd .worktrees/m3-sse && cargo fmt --check && cargo test
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
cd .worktrees/m3-sse && git add Cargo.toml Cargo.lock src/transport/http src/application/mod.rs tests/external_event_stream_api.rs README.md MILESTONE.md docs/superpowers && git commit -m "feat: add external SSE event streams"
```
