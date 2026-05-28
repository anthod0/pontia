# Dashboard Vitest Testing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor dashboard tests to TypeScript/Vitest and replace brittle Svelte source-regex tests with jsdom component tests following Svelte testing documentation.

**Architecture:** Use Vitest as the dashboard test runner through Vite, with jsdom for component tests and Testing Library for user-visible DOM assertions. Keep pure logic tests as fast Vitest unit tests; isolate page component tests by mocking dashboard stores/actions rather than calling the External API.

**Tech Stack:** Svelte 5, Vite, Vitest, jsdom, @testing-library/svelte, @testing-library/user-event, pnpm.

---

### Task 1: Add Vitest test runner

**Files:**
- Modify: `apps/dashboard/package.json`
- Modify: `apps/dashboard/vite.config.ts`
- Generated: `apps/dashboard/pnpm-lock.yaml`

- [ ] Add Vitest/jsdom/Testing Library dev dependencies with `pnpm --dir apps/dashboard add -D vitest jsdom @testing-library/svelte @testing-library/user-event @testing-library/jest-dom`.
- [ ] Add `test: "vitest run"` and `test:watch: "vitest"` scripts.
- [ ] Update Vite config to import `defineConfig` from `vitest/config`, configure `test.environment = 'jsdom'`, and set `resolve.conditions = ['browser']` only when `process.env.VITEST` is present.
- [ ] Run `pnpm --dir apps/dashboard test` and expect existing tests to fail until migrated from `node:test`.

### Task 2: Convert pure unit tests to Vitest TypeScript

**Files:**
- Rename/modify all `apps/dashboard/**/*.test.mjs` pure unit tests to `.test.ts`.
- Modify existing `apps/dashboard/tests/*.test.ts` imports/assertions.

- [ ] Replace `node:test` with `test`/`expect` from `vitest`.
- [ ] Replace `node:assert/strict` assertions with `expect` assertions.
- [ ] Type test fixture helpers where useful, especially DAG/session/task fixture builders.
- [ ] Run targeted Vitest tests for pure units and expect pass.

### Task 3: Replace TasksPage source-regex test with component test

**Files:**
- Replace: `apps/dashboard/tests/tasksTableLayout.test.mjs` -> `apps/dashboard/tests/tasksTableLayout.test.ts`

- [ ] Mock `../src/stores/tasks`, `../src/stores/workspaces`, and `svelte-mini-router` before importing `TasksPage.svelte`.
- [ ] Render `TasksPage.svelte` with seeded writable stores.
- [ ] Assert user-visible table headers and rows are present.
- [ ] Assert structural classes that protect layout (`table-fixed`, `max-w-0`, `truncate`) through rendered DOM rather than source text.
- [ ] Run this test and expect pass.

### Task 4: Replace WorkspacesPage source-regex tests with component tests

**Files:**
- Replace related `.mjs` tests with `apps/dashboard/tests/workspacesPage.test.ts` or equivalent TypeScript tests.

- [ ] Mock `../src/stores/workspaces` with writable stores and vi.fn actions.
- [ ] Render `WorkspacesPage.svelte` with seeded workspace roots, active workspaces, and directory listing data.
- [ ] Assert Root browser and Active workspaces render as DOM sections.
- [ ] Assert directory table has Directory and Action columns only, directory name button opens path, Active button opens registration confirmation with editable default name.
- [ ] Assert active workspace cards include folder preview markup and expose rename action/dialog.
- [ ] Run this test and expect pass.

### Task 5: Final verification

**Files:**
- All dashboard test/config files.

- [ ] Run `pnpm --dir apps/dashboard test`.
- [ ] Run `pnpm --dir apps/dashboard run check`.
- [ ] Fix any TypeScript/Svelte/Vitest issues.
- [ ] Report changed files and verification results.
