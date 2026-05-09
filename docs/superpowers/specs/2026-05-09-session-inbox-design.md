# Session Inbox Design

Date: 2026-05-09

## Summary

Add a session-level inbox so users can submit messages while a session is busy. New user input is represented as an inbox message first. A turn is created only when an inbox message is actually dispatched to the agent runtime.

This keeps the existing meaning of a turn as an execution unit, while allowing users and orchestrators to enqueue or interrupt work without relying on ambiguous TUI/runtime behavior.

## Goals

- Allow messages to be accepted while a session has an active turn.
- Preserve turn as the authoritative execution boundary.
- Support two delivery policies:
  - `after_idle`: run after the current active turn finishes.
  - `interrupt_now`: interrupt the current active turn, then run the new message next.
- Avoid `during_run` injection for now because the control plane cannot know how the agent client handles runtime input during an active turn.
- Keep session and turn state authoritative through existing domain events and projections.
- Provide clear API and UI semantics for queued user intent versus executed turns.

## Non-goals

- No true running-turn input injection.
- No message delivery into an active TUI pane without creating a new turn.
- No background scheduler in the first version.
- No multi-turn concurrent execution in one session.
- No removal of the legacy submit-turn API in this change.

## Core concepts

### Inbox message

An inbox message is persisted user intent. It may be pending, cancelled, superseded, failed, or dispatched. It is not itself an execution record.

### Turn

A turn remains a concrete agent execution unit. A turn has lifecycle events, output, failure, artifacts, and a `turn_id`. A turn is created only when the dispatcher selects an inbox message for execution.

Relationship:

```text
user input -> inbox_message -> turn -> events/output/artifacts
```

## Delivery policies

### `after_idle`

Creates a pending inbox message. If the session is immediately dispatchable, the dispatcher turns it into a turn at once. Otherwise it waits until the session is idle or interrupted and has no `current_turn_id`.

### `interrupt_now`

Creates a pending interrupt message with higher priority than normal queued messages.

Rules:

1. Existing pending `interrupt_now` messages for the same session are marked `superseded`.
2. The newest `interrupt_now` message is the next dispatch candidate.
3. If the session has an active turn, the runtime interrupt flow is requested.
4. After the active turn becomes terminal or interrupted, the newest interrupt message dispatches before ordinary `after_idle` messages.
5. If interrupt is unsupported or fails synchronously with a capability error, the message is marked `failed`; it is not silently downgraded to `after_idle`.

Priority:

```text
newest pending interrupt_now > oldest pending after_idle
```

## API design

### New primary API

```text
POST /external/v1/sessions/{session_id}/inbox/messages
GET  /external/v1/sessions/{session_id}/inbox/messages
GET  /external/v1/sessions/{session_id}/inbox/messages/{message_id}
POST /external/v1/sessions/{session_id}/inbox/messages/{message_id}/cancel
```

Request:

```json
{
  "input": "Please stop and follow this direction instead",
  "delivery_policy": "interrupt_now",
  "metadata": {
    "source": "dashboard"
  }
}
```

`delivery_policy` defaults to `after_idle`.

Response:

```json
{
  "inbox_message": {
    "message_id": "msg_xxx",
    "session_id": "sess_xxx",
    "state": "pending",
    "delivery_policy": "after_idle",
    "input": {
      "summary": "Please continue with the next step"
    },
    "turn_id": null,
    "metadata": {},
    "created_at": "...",
    "updated_at": "..."
  }
}
```

If immediate dispatch succeeds, the response may already show:

```json
{
  "inbox_message": {
    "state": "dispatched",
    "turn_id": "turn_xxx"
  }
}
```

### Legacy API

```text
POST /external/v1/sessions/{session_id}/turns
```

This endpoint is deprecated. It is not extended with inbox behavior. The dashboard and new integrations should use the inbox API. The endpoint may remain with current behavior for compatibility and can be removed later.

`GET /turns` remains the turn-history query API.

## Data model

Add `inbox_messages`:

```sql
CREATE TABLE inbox_messages (
    message_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    state TEXT NOT NULL,
    delivery_policy TEXT NOT NULL,
    input_summary TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    turn_id TEXT,
    superseded_by_message_id TEXT,
    failure_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    dispatched_at TEXT,
    cancelled_at TEXT,

    FOREIGN KEY(session_id) REFERENCES sessions(session_id),
    FOREIGN KEY(turn_id) REFERENCES turns(turn_id)
);

CREATE INDEX idx_inbox_messages_session_state
ON inbox_messages(session_id, state, delivery_policy, created_at, message_id);

CREATE INDEX idx_inbox_messages_turn
ON inbox_messages(turn_id)
WHERE turn_id IS NOT NULL;
```

Inbox state values:

| State | Meaning |
| --- | --- |
| `pending` | Stored and waiting for dispatch. |
| `dispatching` | Selected by dispatcher; turn creation/dispatch is in progress. |
| `dispatched` | Associated with a turn. |
| `cancelled` | Cancelled by user/API before dispatch. |
| `superseded` | Replaced by a newer `interrupt_now` message. |
| `failed` | Could not be dispatched and no turn can represent the failure. |

Normal transitions:

```text
pending -> dispatching -> dispatched
pending -> cancelled
pending -> superseded
pending/dispatching -> failed
```

## Audit events

Inbox messages are stored in their own table, not as a full event-sourced projection. Session and turn state remain authoritative through existing domain events.

The system should still emit audit events into the event timeline when useful:

- `inbox.message_queued`
- `inbox.message_dispatched`
- `inbox.message_cancelled`
- `inbox.message_superseded`
- `inbox.message_failed`

These events must not alter session or turn primary state. They are for observability and UI timeline context.

## Dispatcher design

First version uses synchronous triggers, not a background worker.

Triggers:

1. After creating an inbox message, call `drain_inbox(session_id)`.
2. After ingesting a terminal/interrupted turn event and updating projections, call `drain_inbox(session_id)`.

Dispatch preconditions:

```text
session.state in ('idle', 'interrupted')
session.current_turn_id IS NULL
```

Selection order:

1. Newest `pending interrupt_now` message.
2. Oldest `pending after_idle` message.

Pseudo-flow:

```text
drain_inbox(session_id):
  load session
  if not dispatchable: return no-op

  select next pending message by priority
  if none: return no-op

  conditionally mark selected message dispatching
  if update affected 0 rows: return no-op

  create and dispatch turn from message
  mark message dispatched with turn_id
  write inbox.message_dispatched audit event
```

Concurrency protection:

```sql
UPDATE inbox_messages
SET state = 'dispatching'
WHERE message_id = ?
  AND state = 'pending'
```

Only the caller that updates one row continues. This prevents duplicate turn creation if multiple triggers run concurrently.

## Turn creation refactor

The existing `TurnCommandService::submit_turn` mixes external API validation with turn creation and runtime dispatch. Inbox dispatch should reuse the execution logic without pretending to be a legacy submit-turn request.

Refactor toward an internal method such as:

```rust
create_and_dispatch_turn(session_id, input, metadata) -> TurnView
```

The inbox dispatcher calls this with metadata containing:

```json
{
  "inbox_message_id": "msg_xxx"
}
```

The inbox row also stores `turn_id` after dispatch. This gives bidirectional traceability.

## Interrupt behavior

For `interrupt_now`:

1. Supersede older pending interrupt messages in a transaction.
2. Insert the new message.
3. If there is an active turn, call the existing interrupt flow.
4. If interrupt is unavailable, mark the new message `failed`.
5. If no active turn exists, call `drain_inbox(session_id)` immediately.

No fallback to `after_idle` occurs for failed interrupt requests.

## Idempotency

The new POST API supports `Idempotency-Key`.

Suggested operation key:

```text
submit_inbox_message:{session_id}
```

Repeating the same key returns the original inbox message. If that message has since dispatched, the repeated response reflects the same message with its current state and `turn_id`.

## Web UI changes

Replace the current submit-turn composer with inbox submission behavior.

Expected behavior:

- Busy sessions no longer disable input entirely.
- Default policy is `after_idle`.
- When busy, show actions like:
  - “Queue after current turn”
  - “Interrupt and send next”
- After submit, show pending/dispatched/failed status.
- Turn history continues to come from `/turns`.
- A pending inbox list can be added. First version may minimally refresh session, turns, and events after submission.

## Testing plan

API tests:

1. Idle session + `after_idle` creates inbox message and immediately dispatches a turn.
2. Busy session + `after_idle` creates pending inbox message and does not create a second active turn.
3. Terminal event for current turn triggers dispatch of pending `after_idle`.
4. Busy session + `interrupt_now` supersedes older pending interrupt messages and requests interrupt.
5. Multiple `interrupt_now` submissions leave only the newest pending; older ones are `superseded`.
6. Pending `interrupt_now` dispatches before older `after_idle` messages.
7. Interrupt unavailable marks message `failed`.
8. Idempotent retry returns the same inbox message and does not duplicate dispatch.
9. Cancelling a pending message prevents dispatch.

Dispatcher tests:

1. No-op when session is not `idle` or `interrupted`.
2. No-op when `current_turn_id` is set.
3. Conditional `pending -> dispatching` update prevents duplicate turn creation.
4. Dispatch failure before turn creation marks message `failed`.
5. Runtime dispatch failure after turn creation is represented through the existing turn failure path and the inbox message remains linked to that turn.

Web UI tests:

1. Busy session allows queueing `after_idle`.
2. Busy session exposes interrupt submit option.
3. Submission status displays pending/dispatched/failed state.
4. Turn history still refreshes from `/turns`.

## Open follow-ups

- Decide whether audit event types should extend the existing `EventType` enum or use a more generic non-state-changing event representation.
- Decide whether to add a later repair worker for missed dispatch triggers. The first version intentionally does not include one.
- Later removal plan for deprecated `POST /turns`.
