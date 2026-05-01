# Svelte + Vite WebUI Redesign Todo

Reference design:

- `docs/superpowers/specs/2026-05-01-svelte-vite-webui-redesign.md`

Goal: replace the embedded Rust-string dashboard with a real Svelte + Vite + TypeScript frontend served at `/dashboard`.

---

## Phase 1: Scaffold frontend project and serve a built shell

- [ ] Create `apps/web/package.json` using pnpm.
- [ ] Add Svelte + Vite + TypeScript minimal setup.
- [ ] Add `apps/web/index.html`, `vite.config.ts`, `tsconfig.json`.
- [ ] Create minimal `src/main.ts`, `src/App.svelte`, `src/styles/global.css`.
- [ ] Make the app render a simple `llmparty Dashboard` shell.
- [ ] Configure Vite dev proxy for `/external/*` to `http://127.0.0.1:8080`.
- [ ] Update Rust backend to serve built assets from `apps/web/dist` at `/dashboard` and `/dashboard/assets/*`.
- [ ] Keep or update `tests/web_dashboard.rs` so `/dashboard` returns the Svelte app HTML.
- [ ] Verify:
  - `pnpm --dir apps/web install`
  - `pnpm --dir apps/web build`
  - `cargo test --test web_dashboard`

---

## Phase 2: Add typed API client and shared types

- [ ] Create `apps/web/src/api/types.ts` for `SessionView`, `TurnView`, `EventView`, `ArtifactView`, request/response types.
- [ ] Create `apps/web/src/api/errors.ts` for parsed API errors.
- [ ] Create `apps/web/src/api/client.ts`.
- [ ] Implement bearer token support.
- [ ] Implement JSON envelope parsing.
- [ ] Implement idempotency key generation for mutating requests.
- [ ] Implement functions for:
  - `listSessions`
  - `createSession`
  - `getSession`
  - `listTurns`
  - `submitTurn`
  - `listEvents`
  - `listArtifacts`
  - `discoverArtifacts`
  - `getArtifactContent`
  - `interruptSession`
  - `restartSession`
  - `terminateSession`
- [ ] Verify with typecheck/build:
  - `pnpm --dir apps/web build`

---

## Phase 3: Implement base stores and app shell

- [ ] Create auth/token store with `localStorage` persistence.
- [ ] Create sessions store.
- [ ] Create selected session store.
- [ ] Create session detail store.
- [ ] Create turns store.
- [ ] Create events store.
- [ ] Create artifacts store.
- [ ] Create UI/error/loading store if needed.
- [ ] Build layout components:
  - `AppShell.svelte`
  - `Sidebar.svelte`
  - `StatusBar.svelte`
  - common `ErrorBanner`, `EmptyState`, `LoadingState`, `JsonView`
- [ ] Wire token input/save and global status display.
- [ ] Verify app still builds.

---

## Phase 4: Implement session management UI without SSE

- [ ] Implement `SessionList.svelte`.
- [ ] Implement `CreateSessionForm.svelte`.
- [ ] Implement `SessionDetail.svelte`.
- [ ] Implement `SessionActions.svelte`.
- [ ] Session selection should load:
  - session detail
  - turns
  - events
  - artifacts
- [ ] Lifecycle buttons should call External API and refresh projections.
- [ ] Do not add SSE yet.
- [ ] Manual verification:
  - start backend with `LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run`
  - run Vite dev server
  - save token
  - create session
  - select session
  - interrupt/restart/terminate show clear results/errors

---

## Phase 5: Implement turns and event timeline using polling only

- [ ] Implement `TurnComposer.svelte`.
- [ ] Implement `TurnHistory.svelte`.
- [ ] Implement `LatestReply.svelte`.
- [ ] Implement `EventTimeline.svelte` and `EventItem.svelte`.
- [ ] Use `GET /events` for timeline initially.
- [ ] Use projection state to compute busy/active turn.
- [ ] Disable submit while selected session has `current_turn_id`.
- [ ] After submit turn, refresh selected session, turns, and events.
- [ ] Manual verification:
  - submit a turn
  - busy state appears
  - turn history updates
  - event timeline renders without flicker

---

## Phase 6: Implement SSE stream manager

- [ ] Create `apps/web/src/services/eventStream.ts`.
- [ ] Track current streamed session id.
- [ ] Abort old stream on session switch.
- [ ] Track `lastEventId` per session.
- [ ] Track `seenEventIds` per session.
- [ ] Open stream using `/external/v1/sessions/{session_id}/events/stream?after=...` when cursor exists.
- [ ] Deduplicate incoming events by `event_id`.
- [ ] Append new events to timeline without full rerender flicker.
- [ ] On `turn.output`, update latest reply and optionally debounce turn refresh.
- [ ] On terminal turn events, refresh session detail and turns only; do not recursively reselect session.
- [ ] On session state events, refresh session detail and session list.
- [ ] Expose connection status in UI.
- [ ] Manual verification:
  - Network tab should show one active stream per selected session.
  - terminal events should not cause endless reconnect.
  - timeline should not duplicate events after reconnect.
  - switching sessions aborts the previous stream.

---

## Phase 7: Implement artifact browser

- [ ] Implement `ArtifactBrowser.svelte`.
- [ ] Implement `ArtifactContentViewer.svelte`.
- [ ] Add artifact discover button.
- [ ] Load artifact list for selected session.
- [ ] Load artifact content via External API.
- [ ] Show text content when possible; show safe fallback for binary/large/error cases.
- [ ] Manual verification:
  - discover artifacts
  - select artifact
  - content loads or clear API error appears

---

## Phase 8: Replace old embedded dashboard

- [ ] Remove or simplify `src/transport/http/dashboard.rs` so it no longer contains the old full HTML/JS app.
- [ ] Ensure `/dashboard` serves the built Svelte app.
- [ ] Ensure asset paths work after `pnpm --dir apps/web build`.
- [ ] Update `src/transport/http/mod.rs` if routes need static asset handling.
- [ ] Update `tests/web_dashboard.rs` assertions to match new app shell, not old embedded strings.
- [ ] Verify:
  - `pnpm --dir apps/web build`
  - `cargo test --test web_dashboard`

---

## Phase 9: Documentation and cleanup

- [ ] Update `apps/web/README.md` for Svelte/Vite workflow.
- [ ] Update root `README.md` dashboard section if commands change.
- [ ] Document dev mode:
  - backend: `LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run`
  - frontend: `pnpm --dir apps/web dev`
- [ ] Document built mode:
  - `pnpm --dir apps/web build`
  - `LLMPARTY_EXTERNAL_API_TOKEN=dev-token cargo run`
  - open `/dashboard`
- [ ] Remove stale references to zero-build dashboard.
- [ ] Check no generated build artifacts are accidentally committed unless intentionally required.

---

## Phase 10: Final verification

- [ ] Run frontend checks:
  - `pnpm --dir apps/web build`
- [ ] Run Rust checks:
  - `cargo fmt --check`
  - `cargo test --test web_dashboard`
  - `cargo test`
- [ ] Manual smoke test:
  - open `/dashboard`
  - save token
  - create session
  - submit turn
  - observe events
  - verify no timeline flicker
  - verify one SSE stream per selected session
  - verify terminal turn event does not trigger endless replay
  - discover/read artifacts
  - terminate session

---

## Suggested session split

Recommended implementation split across agent sessions:

1. Session A: Phases 1-4
2. Session B: Phases 5-6
3. Session C: Phases 7-10

If done in one long session, still complete and verify each phase before moving to the next.
