# M3 SSE External Event Stream Design

## Goal

Complete Milestone 3 by adding a read-only External Event Stream API based on Server-Sent Events (SSE), without adding WebSocket support or introducing a new domain state source.

## Scope

Add two authenticated stream endpoints parallel to the existing polling endpoints:

- `GET /external/v1/sessions/{session_id}/events/stream`
- `GET /external/v1/sessions/{session_id}/turns/{turn_id}/events/stream`

Both endpoints stream persisted domain events from the existing SQLite `events` table. They do not write events, mutate projections, read runtime/client internals, or replace the existing polling APIs.

## API

Authentication remains `Authorization: Bearer <external_api_token>`.

Query parameters:

- `after=<event_id>`: optional resume cursor. When present, stream only events after that event within the requested session or turn scope. When absent, stream from the beginning of the scope.

Each SSE item uses:

- `id`: domain `event_id`
- `event`: `domain_event`
- `data`: JSON serialized `EventView`

Example:

```text
id: evt_abc
event: domain_event
data: {"event_id":"evt_abc","session_id":"sess_abc","turn_id":null,"source":"runtime_manager","type":"session.ready","time":"...","payload":{}}
```

## Architecture

The first implementation is lightweight polling SSE. The handler validates authentication and resource existence, converts the optional external cursor to an internal `events.rowid` cursor, then repeatedly queries the event store for rows after that cursor. New rows are emitted as SSE events. When no rows are available, the stream sleeps briefly and emits periodic keepalive comments.

This intentionally keeps SQLite/the event store as the only authoritative event source. It avoids adding an in-memory broadcaster in M3, so reconnect and server restart semantics remain DB-backed.

## Components

- `src/application/mod.rs`
  - Add cursor-aware event query methods that return event row cursors with `EventView` values.
  - Validate that `after` belongs to the requested session/turn scope.
- `src/transport/http/external.rs`
  - Add SSE handlers, query parameter parsing, authentication reuse, stream construction, and SSE error mapping.
- `src/transport/http/mod.rs`
  - Register the new routes.
- `tests/external_event_stream_api.rs`
  - Cover authentication, session stream, turn stream filtering, cursor resume, and invalid cursor behavior.
- `README.md` and `MILESTONE.md`
  - Document M3 usage and mark M3 complete after verification.

## Cursor and Resume Semantics

`event_id` is the external cursor. Internally, the service resolves it to the matching SQLite `rowid` in the requested scope.

- No `after`: start after rowid `0`, so existing events are streamed from the beginning.
- Valid `after`: start after that rowid.
- Unknown `after` or cursor outside requested scope: return `400 invalid_request` before starting SSE.

Clients can reconnect with the last received SSE `id` as `after`.

## Error Handling

Before the SSE response starts, errors use the existing JSON error envelope:

- `401 authentication_failed` for missing/wrong token.
- `404 not_found` for missing session or turn.
- `400 invalid_request` for invalid/out-of-scope cursor.

Once streaming starts, transient DB query errors end the stream instead of emitting forged domain events.

## Testing

Use TDD. Add failing integration tests before production changes:

1. unauthenticated session stream is rejected;
2. session stream emits persisted events as SSE frames;
3. `after` cursor resumes after the requested event;
4. turn stream emits only events for the requested turn;
5. invalid cursor returns `400 invalid_request`;
6. existing polling event APIs still pass unchanged.
