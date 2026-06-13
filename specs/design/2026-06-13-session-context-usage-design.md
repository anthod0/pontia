# Session Context Usage Design

Date: 2026-06-13

## Goal

Expose agent client context-window usage in the dashboard as a session-level state.

The first implementation should focus on the pi client and the session detail UI. The design keeps client-specific extraction inside the client extension and exposes only generic Pontia state through the External API.

## Non-goals

- Do not parse tmux panes, TUI screen text, runtime logs, or workspace files for context usage.
- Do not make the dashboard read SQLite, runtime directories, Internal API, or agent client files directly.
- Do not model per-turn historical charts in the first version.
- Do not fabricate exact usage when the agent client does not provide it.

## Architecture Summary

Data flow:

```text
agent client extension
  -> POST /internal/v1/events session.context_usage_updated
  -> events table stores the raw fact
  -> projection updates sessions.metadata.context_usage
  -> /external/v1/sessions exposes SessionView.context_usage
  -> dashboard reads External API snapshot and refreshes after SSE events
```

`events` remains the factual event log. The frontend reads the projected External API view, not `events` directly.

## Capability Model

Add context-usage support to the agent client/session capability model.

Recommended shape:

```rust
pub enum ContextUsageCapability {
    Unsupported,
    Estimated,
    Exact,
}
```

Serialized External API values should be snake_case strings:

```json
{
  "capabilities": {
    "context_usage": "unsupported"
  }
}
```

Allowed values:

- `unsupported`: the client cannot provide usage.
- `estimated`: the client/plugin can estimate usage but it is not authoritative.
- `exact`: the client/plugin receives exact usage from the agent client/model runtime.

For the pi client, set this according to what the pi extension can actually read. If the current pi hook payload does not expose context/token usage, use `unsupported` until proven otherwise.

## Domain Event

Add a new event type:

```text
session.context_usage_updated
```

Properties:

- `source`: should normally be `agent_client`.
- `client_type`: e.g. `pi`.
- `session_id`: required.
- `turn_id`: optional. It may be present for a usage observation during a concrete turn, but first version only projects session-level latest usage.
- Does not change session or turn lifecycle state.
- Does not require `turn_id`.

Example Internal Event API request:

```json
{
  "event_id": "evt_...",
  "session_id": "sess_...",
  "turn_id": "turn_...",
  "source": "agent_client",
  "client_type": "pi",
  "type": "session.context_usage_updated",
  "time": "2026-06-13T00:00:00.000Z",
  "seq": null,
  "payload": {
    "context_usage": {
      "used_tokens": 42000,
      "max_tokens": 128000,
      "remaining_tokens": 86000,
      "usage_ratio": 0.328125,
      "confidence": "exact"
    },
    "model": "example-model"
  }
}
```

## Context Usage Model

Generic model exposed by Pontia:

```ts
interface ContextUsageView {
  used_tokens: number | null;
  max_tokens: number | null;
  remaining_tokens: number | null;
  usage_ratio: number | null; // 0..1
  input_tokens: number | null;
  output_tokens: number | null;
  cache_tokens: number | null;
  confidence: 'exact' | 'estimated' | 'unknown';
  observed_at: string;
}
```

Validation rules:

- `payload` must be a JSON object.
- `payload.context_usage` must be a JSON object.
- Numeric token fields must be non-negative integers when present.
- `usage_ratio` must be between `0` and `1` when present.
- `confidence` must be `exact`, `estimated`, or `unknown`.
- `payload.context_usage.model` is not supported; optional `payload.model` must be a string or null when present and is projected as session-level `model`.
- `observed_at` should be set by the server projection from event `occurred_at`, not trusted from client payload.
- Client-specific raw fields should not leak into the External API view. If raw details are needed for diagnostics, keep them under event payload metadata only.

## Persistence and Projection

Raw event:

- Insert into `events` as normal.

Projected state:

- Store latest session context usage in `sessions.metadata.context_usage`.
- Do not add dedicated SQL columns in the first version unless implementation strongly benefits from it.
- Do not change existing migrations. If a migration is needed, append a new numbered migration only.

Projected JSON example:

```json
{
  "context_usage": {
    "used_tokens": 42000,
    "max_tokens": 128000,
    "remaining_tokens": 86000,
    "usage_ratio": 0.328125,
    "input_tokens": null,
    "output_tokens": null,
    "cache_tokens": null,
    "confidence": "exact",
    "observed_at": "2026-06-13T00:00:00.000Z"
  },
  "model": "example-model"
}
```

Projection behavior:

- On `session.context_usage_updated`, merge/replace `sessions.metadata.context_usage` with the normalized view.
- Preserve other existing `sessions.metadata` keys.
- Do not update `current_turn_id`, session state, or turn state.
- If later per-turn usage is needed, add turn projection separately. First version is session-only.

## External API

Add explicit field to `SessionView`:

```rust
pub model: Option<String>
pub context_usage: Option<ContextUsageView>
```

Dashboard TypeScript type:

```ts
export interface SessionView {
  // existing fields...
  model: string | null;
  context_usage: ContextUsageView | null;
}
```

`context_usage` should be parsed from `sessions.metadata.context_usage`, and `model` from `sessions.metadata.model`, by the query/view layer. Do not require frontend components to inspect `metadata` directly.

Existing session endpoints should include the field:

- `GET /external/v1/sessions`
- `GET /external/v1/sessions/:session_id`

SSE:

- The normal dashboard event stream can emit the new session event.
- Frontend should treat SSE as an invalidation signal and refresh the session snapshot, or patch from event payload if the store already has robust patching.
- External API snapshot remains authoritative for initial page load and recovery.

## pi Client Extension

The pi extension should be the only pi-specific place that reads context usage.

Responsibilities:

1. Inspect pi hook event payloads or pi extension APIs for real usage data.
2. Convert client-specific shape into Pontia `context_usage`.
3. Report `session.context_usage_updated` through the Internal Event API.
4. If no reliable usage source exists, do not report fake data; leave capability as `unsupported`.

Suggested builder:

```ts
export function buildSessionContextUsageUpdatedEvent(
  context: TurnContext,
  usage: ContextUsagePayload,
  model?: string | null,
): InternalEvent
```

Suggested extraction function:

```ts
function contextUsageFromPiEvent(event: unknown): ContextUsageObservation | undefined {
  // Read only documented/observed pi hook/API fields.
  // Return undefined if no usage exists.
  // Return model separately from context_usage when observed.
}
```

Potential hook points:

- `message_update`: if usage is available during streaming.
- `message_end`: if usage becomes available at assistant message end.
- `agent_end`: final observation for the turn.

Avoid emitting excessive events. If streaming usage is available, throttle or emit only when values change meaningfully.

## Dashboard UX

First version target: Session detail page.

Display states:

- Capability `unsupported`: hide the card or show a small “Context usage not supported by this client”.
- Capability `exact`/`estimated` and no usage yet: show “Waiting for context usage...”.
- Usage with `max_tokens`: show progress bar and `used / max` plus percentage.
- Usage without `max_tokens`: show `used_tokens` only.

Suggested thresholds:

- `< 70%`: normal.
- `70% - 90%`: warning.
- `> 90%`: danger.

Example labels:

```text
Context 42k / 128k · 33% · exact
```

## Tests

Backend:

- Event type parsing/serialization for `session.context_usage_updated`.
- Internal Event API accepts valid context usage.
- Internal Event API rejects invalid values: negative tokens, ratio outside `0..1`, unknown confidence.
- Projection updates `sessions.metadata.context_usage` without changing session lifecycle state.
- External API returns `session.context_usage` for list and detail endpoints.
- Capability serialization includes `context_usage`.

pi client:

- Event builder creates valid `session.context_usage_updated` event.
- Extraction returns `undefined` when no usage is available.
- Reporter posts the usage event when extraction succeeds.
- No usage event is reported for unsupported/missing data.

Dashboard:

- Type definitions include `ContextUsageView` and capability field.
- Session detail renders unsupported, waiting, and populated states.
- SSE/session-event invalidation refreshes or patches session usage.

## Implementation Notes

- Respect migration rule: never edit old migrations.
- Keep client-specific fields out of generic domain events and External API view models.
- Dashboard must use `/external/v1/*` only.
- Internal Event API remains the only write path for agent-client observed facts.
- If using `sessions.metadata`, be careful to merge existing metadata instead of replacing it wholesale.
