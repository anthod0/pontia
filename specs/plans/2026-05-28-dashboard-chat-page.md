# Dashboard Chat Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a user-friendly `/dashboard/chat` page for session conversations while keeping the existing `/dashboard/sessions` advanced console unchanged.

**Architecture:** Introduce reusable session chat helpers and Svelte components that consume existing External API session stores. The page maps turns to user/assistant chat messages and submits follow-up input through the existing inbox endpoint with `after_idle` delivery.

**Tech Stack:** Svelte 5, Vite, TypeScript, Tailwind CSS 4, shadcn-svelte/Svelte AI Elements registry components, existing `/external/v1/*` API client and stores.

---

## Files

- Create `apps/dashboard/src/lib/session-chat/sessionChat.ts`: pure helpers for session title/filtering, turn-to-message mapping, and send enablement.
- Create `apps/dashboard/src/lib/session-chat/sessionChat.test.mjs`: node tests for reusable chat logic.
- Create `apps/dashboard/src/lib/components/session-chat/SessionList.svelte`: reusable friendly session list.
- Create `apps/dashboard/src/lib/components/session-chat/SessionConversation.svelte`: reusable message display.
- Create `apps/dashboard/src/lib/components/session-chat/SessionMessageComposer.svelte`: reusable prompt input wrapper.
- Create `apps/dashboard/src/pages/ChatPage.svelte`: page composition and store/API orchestration.
- Modify `apps/dashboard/src/routes.ts`: add `/chat` route.
- Modify `apps/dashboard/src/components/layout/AppSidebar.svelte`: add Chat nav entry.
- Add Svelte AI Elements minimal components under `apps/dashboard/src/lib/components/ai-elements/` via registry if compatible.

## Tasks

### Task 1: Reusable chat logic

- [ ] Write failing tests covering active/all session filtering, turn-to-message mapping, failures, and send gating.
- [ ] Run `node --experimental-strip-types apps/dashboard/src/lib/session-chat/sessionChat.test.mjs` and verify failure.
- [ ] Implement `sessionChat.ts` minimally.
- [ ] Re-run the test and verify pass.

### Task 2: Import minimal Svelte AI Elements UI

- [ ] Run registry add commands for `message`, `prompt-input`, and `conversation` using pnpm.
- [ ] Inspect generated files and adapt only if Vite/Svelte aliases require it.
- [ ] Run `pnpm --dir=apps/dashboard run check` to verify compatibility.

### Task 3: Reusable Svelte components

- [ ] Build `SessionList.svelte`, `SessionConversation.svelte`, and `SessionMessageComposer.svelte` around the pure helpers and AI Elements primitives.
- [ ] Keep components API-oriented: props in, callbacks out, no direct API calls except page-level orchestration.
- [ ] Run dashboard check.

### Task 4: `/chat` page and navigation

- [ ] Compose page with existing `sessions` and `sessionDetail` stores.
- [ ] Load sessions on mount, auto-select newest active session when available.
- [ ] Submit messages through `submitInboxMessage(..., { delivery_policy: 'after_idle', metadata: { source: 'dashboard_session_chat' } })`.
- [ ] Add route `/chat` and sidebar item named `Chat`.
- [ ] Run targeted tests and `pnpm --dir=apps/dashboard run check`.
