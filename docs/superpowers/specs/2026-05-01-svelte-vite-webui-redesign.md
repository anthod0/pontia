# Svelte + Vite WebUI Redesign

## 1. Goal

Rewrite the current llmparty WebUI as a maintainable frontend application using Svelte, Vite, TypeScript, and plain CSS.

The existing `/dashboard` implementation is an MVP-only zero-build page embedded in `src/transport/http/dashboard.rs` as a Rust string. It should be treated as deprecated and replaced by an independent frontend project under `apps/web`.

The new WebUI remains a consumer of llmparty's External API and External Event Stream API. It must not become a new control-plane source of truth.

## 2. Non-goals

This redesign does not introduce:

- React, Vue, SvelteKit, or a heavy UI framework.
- Direct SQLite reads from the browser.
- Direct runtime, tmux, pi TUI, log, or workspace access from the browser.
- New control-plane semantics outside the External API.
- WebSocket. The existing SSE event stream remains the realtime transport.
- Multi-user auth, OAuth, RBAC, or tenant isolation.

## 3. Technology Stack

Use:

- Svelte
- Vite
- TypeScript
- Plain CSS with CSS variables
- pnpm as the package manager

Avoid initially:

- SvelteKit
- Tailwind
- Large component libraries
- Global state libraries unless Svelte stores become insufficient
- OpenAPI codegen unless the API surface grows substantially

Rationale:

- The WebUI is a client-side control panel with moderate state complexity.
- Svelte provides component structure and reactive state without React's runtime and ecosystem weight.
- Vite provides a simple frontend build and dev server.
- Build output can be served as static assets by the Rust backend.

## 4. Product Scope

The first rewritten WebUI should support at least the current dashboard feature set:

- Store and use an External API bearer token.
- List sessions.
- Create a session with `client_type`, `workspace`, and optional initial task.
- Select and inspect one session.
- Submit turns to the selected session.
- Display active/busy turn state.
- Display latest `turn.output` reply.
- Display turn history.
- Display session event timeline.
- Consume SSE from `/external/v1/sessions/{session_id}/events/stream`.
- Discover and list artifacts.
- Read artifact content through the External API.
- Run session lifecycle actions: interrupt, restart, terminate.
- Show clear API and connection errors.

Future-friendly structure should allow later additions:

- Better event filtering/search.
- Artifact previews by content type.
- Runtime diagnostics exposed by future External API endpoints.
- Approval / human-in-the-loop UI if the domain model later supports it.
- Multiple client types beyond generic and pi.

## 5. Architecture Principles

### 5.1 External API remains authoritative

The browser reads and writes only through:

- `/external/v1/*`
- `/external/v1/*/events/stream`

It must not read:

- SQLite files
- runtime bindings directly
- tmux state
- pi internals
- workspace files directly
- hook logs directly

If the UI needs new data, the backend should expose it through an explicit External API endpoint first.

### 5.2 SSE is a realtime optimization

SSE events improve freshness but do not replace projection reads.

The UI should use HTTP query endpoints for authoritative snapshots:

- `GET /external/v1/sessions`
- `GET /external/v1/sessions/{session_id}`
- `GET /external/v1/sessions/{session_id}/turns`
- `GET /external/v1/sessions/{session_id}/events`
- `GET /external/v1/sessions/{session_id}/artifacts`

SSE should:

- Append events to the timeline.
- Update lightweight UI state when safe, such as latest `turn.output`.
- Trigger targeted projection refreshes after terminal or state-changing events.

### 5.3 State synchronization is explicit

The current dashboard bug class comes from implicit coupling between session selection, refresh, and SSE reconnects.

The rewrite should make these flows explicit:

- Session selection owns snapshot loading and SSE stream switching.
- SSE stream management owns cursor, deduplication, reconnect, and abort.
- Projection refreshes should not reset SSE cursor.
- Terminal turn events should refresh session/turn state, not recursively reselect the session.

## 6. Frontend Project Structure

Recommended structure:

```text
apps/web/
  package.json
  index.html
  vite.config.ts
  tsconfig.json

  src/
    main.ts
    App.svelte

    api/
      client.ts
      types.ts
      errors.ts

    stores/
      auth.ts
      sessions.ts
      selection.ts
      sessionDetail.ts
      turns.ts
      events.ts
      artifacts.ts
      connection.ts
      ui.ts

    services/
      eventStream.ts
      refreshCoordinator.ts

    components/
      layout/
        AppShell.svelte
        Sidebar.svelte
        StatusBar.svelte

      sessions/
        SessionList.svelte
        CreateSessionForm.svelte
        SessionDetail.svelte
        SessionActions.svelte

      turns/
        TurnComposer.svelte
        TurnHistory.svelte
        LatestReply.svelte

      events/
        EventTimeline.svelte
        EventItem.svelte

      artifacts/
        ArtifactBrowser.svelte
        ArtifactContentViewer.svelte

      common/
        JsonView.svelte
        ErrorBanner.svelte
        EmptyState.svelte
        LoadingState.svelte

    styles/
      global.css
```

The exact file list may change during implementation, but the boundaries should remain:

- API access in `api/`
- shared state in `stores/`
- long-running side effects in `services/`
- UI rendering in `components/`

## 7. Layout

Initial layout should remain a dashboard rather than a multi-page app:

```text
┌─────────────────────────────────────────────┐
│ Header: llmparty / token / connection state │
├───────────────┬─────────────────────────────┤
│ Session list  │ Selected session             │
│ Create form   │ Turn composer + latest reply │
│               │ Turn history                 │
│               │ Event timeline               │
│               │ Artifact browser             │
└───────────────┴─────────────────────────────┘
```

Do not introduce routing in the first rewrite unless needed. A single application view keeps the first migration smaller and easier to verify.

## 8. State Model

Use Svelte stores for application state.

Recommended stores:

### 8.1 `auth`

- `token`
- load/save token to `localStorage`

### 8.2 `sessions`

- `sessions`
- `loading`
- `error`
- `loadSessions()`

### 8.3 `selection`

- `selectedSessionId`
- `selectSession(sessionId)` orchestration entry point

### 8.4 `sessionDetail`

- `session`
- `loading`
- `error`
- `refreshSession(sessionId)`

### 8.5 `turns`

- `turns`
- derived `activeTurn`
- derived `latestOutput`
- `loadTurns(sessionId)`
- `submitTurn(sessionId, input, metadata?)`

### 8.6 `events`

Maintain event state per session:

- `eventsBySession`
- `seenEventIdsBySession`
- `lastEventIdBySession`

Rules:

- Event append must be idempotent by `event_id`.
- `lastEventId` is updated only after a valid event is accepted.
- Loading historical events should seed `seenEventIds` and cursor state.

### 8.7 `artifacts`

- `artifacts`
- `selectedArtifact`
- `artifactContent`
- `loadArtifacts(sessionId)`
- `discoverArtifacts(sessionId)`
- `loadArtifactContent(artifactId)`

### 8.8 `connection`

- `sseStatus`: `idle | connecting | open | reconnecting | closed | error`
- `reconnectCount`
- `lastError`
- current streamed `sessionId`

## 9. API Client

Create a typed API client in `src/api/client.ts`.

Responsibilities:

- Add `Authorization: Bearer <token>`.
- Add `Idempotency-Key` for mutating requests.
- Parse llmparty JSON envelopes.
- Convert error envelopes into typed errors.
- Support artifact content reads, which are not normal JSON envelopes.

Example API functions:

```ts
listSessions(): Promise<SessionView[]>
createSession(input: CreateSessionInput): Promise<CreateSessionResult>
getSession(sessionId: string): Promise<SessionView>
listTurns(sessionId: string): Promise<TurnView[]>
submitTurn(sessionId: string, input: SubmitTurnInput): Promise<TurnView>
listEvents(sessionId: string): Promise<EventView[]>
listArtifacts(sessionId: string): Promise<ArtifactView[]>
discoverArtifacts(sessionId: string): Promise<ArtifactView[]>
getArtifactContent(artifactId: string): Promise<ArtifactContent>
interruptSession(sessionId: string): Promise<SessionView | unknown>
restartSession(sessionId: string): Promise<SessionView | unknown>
terminateSession(sessionId: string): Promise<SessionView | unknown>
```

Types should mirror the External API view models documented in `spec/06-control-plane-external-api-v1.md`.

## 10. SSE Stream Management

Implement SSE management in `src/services/eventStream.ts`.

Responsibilities:

- Open stream for the selected session.
- Abort the previous stream when session changes.
- Use per-session `lastEventId` as the `?after=` cursor.
- Deduplicate by `event_id`.
- Update connection status.
- Reconnect on transient failures.
- Stop reconnecting when the selected session changes or the user logs out.

Important rule:

> Refreshing selected session projections must not reset the SSE cursor.

Event handling policy:

- `turn.output`
  - append to timeline
  - update latest reply if payload contains output summary
  - optionally refresh turns with debounce

- `turn.completed` / `turn.failed` / `turn.interrupted` / `turn.cancelled`
  - append to timeline
  - refresh selected session
  - refresh turns
  - do not call `selectSession()` recursively
  - do not reset `lastEventId`

- `session.ready` / `session.started` / `session.exited` / `session.error`
  - append to timeline
  - refresh selected session and session list

- artifact-related future events
  - append to timeline
  - refresh artifact list if applicable

## 11. Refresh Coordination

Create a small refresh coordinator to avoid repeated full refreshes.

It should provide debounced or coalesced refresh operations such as:

- `refreshSelectedSession()`
- `refreshTurns()`
- `refreshSessionList()`
- `refreshArtifacts()`

This prevents rapid event bursts from causing UI flicker or redundant HTTP requests.

## 12. Command Flow

### 12.1 Create session

1. POST create session.
2. Refresh session list.
3. Select created session.
4. Load session snapshot, turns, events, artifacts.
5. Open SSE stream for that session.

### 12.2 Select session

1. Set `selectedSessionId`.
2. Abort previous SSE stream.
3. Load authoritative snapshot data for the new session.
4. Seed events and cursor from historical events.
5. Open SSE stream from known cursor.

### 12.3 Submit turn

1. Validate selected session and non-empty input.
2. POST submit turn.
3. Refresh selected session and turns.
4. Let SSE provide subsequent `turn.started`, `turn.output`, and terminal events.
5. Disable submit while the selected session has an active turn.

### 12.4 Terminal turn event

1. Append/dedupe event.
2. Refresh selected session and turns.
3. Recompute busy state from projection.
4. Do not reselect session and do not reopen SSE from scratch.

## 13. Rust Backend Integration

The Rust backend should serve the built frontend instead of returning embedded HTML.

Expected behavior:

- `/dashboard` serves the frontend entry HTML.
- `/dashboard/` also serves the frontend entry HTML.
- `/dashboard/assets/*` serves static assets from `apps/web/dist/assets`.

The old embedded dashboard can be removed or retained only as a temporary fallback during migration. Once the Svelte app is served successfully, the Rust string dashboard should be deleted to avoid two divergent UIs.

Development mode:

- Run backend normally with `cargo run`.
- Run frontend dev server with `pnpm --dir apps/web dev`.
- Vite proxies `/external/*` to the backend.

Production/local built mode:

```bash
pnpm --dir apps/web build
LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run
```

Then open:

```text
http://127.0.0.1:8080/dashboard
```

## 14. Testing Strategy

### 14.1 Frontend checks

Initial checks:

```bash
pnpm --dir apps/web typecheck
pnpm --dir apps/web build
```

Later checks:

- Vitest for API client and SSE logic.
- Playwright for browser workflow tests.

High-value unit tests:

- API envelope success/error parsing.
- Event deduplication by `event_id`.
- Cursor update behavior.
- Terminal event does not reset cursor.
- Session switch aborts previous stream.
- Historical event load seeds dedupe state.

### 14.2 Backend checks

Keep or update:

```bash
cargo test --test web_dashboard
```

The test should verify:

- `/dashboard` returns HTML.
- Built asset routes are served.
- The dashboard shell references the frontend bundle.

Existing API tests should continue to pass.

### 14.3 Manual acceptance

Manual acceptance should cover:

1. Start backend with `LLMPARTY_EXTERNAL_API_TOKEN`.
2. Open `/dashboard`.
3. Enter token.
4. Create a `generic` or `pi` session.
5. Submit a turn.
6. Observe event timeline without flicker or duplicate event growth.
7. Observe latest reply from `turn.output`.
8. Observe submit disabled while busy and re-enabled after terminal event.
9. Browse artifacts and read artifact content.
10. Run restart / terminate actions and verify state updates.

For real pi validation, continue using the README flow with `LLMPARTY_INTERNAL_EVENT_URL` and the pi extension.

## 15. Migration Plan Summary

Implementation should proceed in small steps:

1. Scaffold `apps/web` with Svelte, Vite, TypeScript, and pnpm.
2. Add typed API client and External API types.
3. Implement stores and refresh coordinator.
4. Implement dashboard shell and session list/detail.
5. Implement turn composer/history/latest reply.
6. Implement event timeline with historical load and SSE dedupe.
7. Implement artifact browser/content viewer.
8. Implement lifecycle actions.
9. Update Rust backend to serve `apps/web/dist` at `/dashboard`.
10. Replace old `dashboard.rs` embedded HTML.
11. Update tests and README.

## 16. Acceptance Criteria

The redesign is complete when:

- The old embedded dashboard is no longer the active UI.
- `/dashboard` serves the Svelte/Vite app.
- The WebUI can perform all previous dashboard operations through External API only.
- SSE reconnect and terminal turn events do not cause repeated history replay or timeline flicker.
- Event timeline deduplicates by `event_id`.
- Session switching aborts the old stream and starts the new stream with the correct cursor.
- Submit button state reflects the authoritative selected session/turn projection.
- Frontend build and typecheck pass.
- Relevant Rust tests pass.
