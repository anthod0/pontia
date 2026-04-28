# M1 tmux Runtime Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Milestone 1 by making the runtime manager use real tmux sessions for session lifecycle, termination, restart, logs, workspace/env metadata, and crash observation.

**Architecture:** The Control Plane keeps External API state event-driven. RuntimeManager owns real tmux session lifecycle and internal binding metadata. pi/generic session runtimes are long-lived tmux sessions; M0 pi RPC shortcut must not be the M1 runtime authority.

**Tech Stack:** Rust 2024, Axum, SQLx/SQLite, real `tmux` CLI, integration tests, TDD.

---

## File Structure

- Modify: `.worktrees/m1-tmux-runtime/src/runtime/mod.rs`
  - Replace placeholder `GenericRuntimeManager` behavior with a tmux-backed runtime manager.
  - Add tmux session naming, workspace preparation, command construction, log path, environment injection, has/kill/create helpers, and runtime metadata.
- Modify: `.worktrees/m1-tmux-runtime/src/application/mod.rs`
  - Store richer runtime binding metadata.
  - Add runtime observation service that maps missing tmux runtime to `session.error` and active `turn.failed`.
  - Ensure terminate/restart call real tmux behavior.
  - Stop treating pi RPC as session runtime authority; leave M0 turn shortcut only if needed for existing M0 behavior.
- Modify: `.worktrees/m1-tmux-runtime/src/config.rs`
  - Add runtime env vars only if implementation needs configurable tmux command/client command/workspace root.
- Create: `.worktrees/m1-tmux-runtime/tests/tmux_runtime_m1.rs`
  - Real tmux integration tests. No fake tmux. Tests fail clearly if tmux is unavailable.
- Modify: `.worktrees/m1-tmux-runtime/tests/runtime_lifecycle_api.rs`
  - Adjust existing lifecycle tests to expect real tmux side effects where appropriate.
- Modify: `.worktrees/m1-tmux-runtime/README.md`
  - Document M1 tmux dependency and verification commands.
- Modify: `.worktrees/m1-tmux-runtime/MILESTONE.md`
  - Mark M1 done only after verification passes.

## Task 1: Real tmux session creation and binding metadata

**Files:**
- Modify: `.worktrees/m1-tmux-runtime/src/runtime/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/src/application/mod.rs`
- Create: `.worktrees/m1-tmux-runtime/tests/tmux_runtime_m1.rs`

- [ ] **Step 1: Write failing test**

Add `create_generic_session_creates_real_tmux_runtime` that:
- asserts `tmux -V` succeeds;
- creates a generic session through External API;
- queries `runtime_bindings` directly;
- asserts metadata contains `backend = "tmux"`, `tmux_session`, `workspace`, `log_path`, `started_at`, `restart_count = 0`;
- runs `tmux has-session -t <tmux_session>` and expects success;
- cleans up by killing the tmux session.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 create_generic_session_creates_real_tmux_runtime -- --test-threads=1`
Expected: FAIL because runtime binding is currently `generic:<session_id>` and no tmux session exists.

- [ ] **Step 3: Implement minimal code**

In `src/runtime/mod.rs`:
- make `start_session` create a real tmux session named `llmparty_<sanitized_session_id>`;
- create workspace directory from request workspace or temp default;
- create `.llmparty/runtime.log` path;
- start a safe long-running shell command in tmux;
- inject env vars `LLMPARTY_SESSION_ID`, `LLMPARTY_CLIENT_TYPE`, `LLMPARTY_WORKSPACE`;
- return `runtime_kind = "tmux"`, `runtime_ref = <tmux_session>`, and metadata with backend/session/workspace/log/restart count.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 create_generic_session_creates_real_tmux_runtime -- --test-threads=1`
Expected: PASS.

## Task 2: terminate kills real tmux runtime

**Files:**
- Modify: `.worktrees/m1-tmux-runtime/src/runtime/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/src/application/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/tests/tmux_runtime_m1.rs`

- [ ] **Step 1: Write failing test**

Add `terminate_session_kills_real_tmux_runtime` that creates a session, captures `tmux_session`, calls `DELETE /external/v1/sessions/{session_id}`, asserts session state `exited`, then asserts `tmux has-session -t <tmux_session>` fails.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 terminate_session_kills_real_tmux_runtime -- --test-threads=1`
Expected: FAIL because current terminate is no-op.

- [ ] **Step 3: Implement minimal code**

Make `RuntimeControlService::terminate_session` load binding metadata/ref and call `tmux kill-session -t <tmux_session>` through RuntimeManager. Treat missing session during terminate as already stopped, then emit `session.exited`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 terminate_session_kills_real_tmux_runtime -- --test-threads=1`
Expected: PASS.

## Task 3: restart replaces real tmux runtime cycle

**Files:**
- Modify: `.worktrees/m1-tmux-runtime/src/runtime/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/src/application/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/tests/tmux_runtime_m1.rs`

- [ ] **Step 1: Write failing test**

Add `restart_replaces_tmux_runtime_and_returns_idle` that creates a session, captures binding metadata, calls restart, captures new metadata, asserts state `idle`, same tmux session name exists, `restart_count = 1`, and old runtime was killed/recreated by checking `started_at` changed.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 restart_replaces_tmux_runtime_and_returns_idle -- --test-threads=1`
Expected: FAIL because restart only rewrites placeholder binding.

- [ ] **Step 3: Implement minimal code**

Make `restart_session` kill existing tmux session if present, create a new one, preserve/increment restart count from prior metadata, update binding metadata, and keep event sequence `session.starting`, `session.started`, `session.ready`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 restart_replaces_tmux_runtime_and_returns_idle -- --test-threads=1`
Expected: PASS.

## Task 4: crash observation maps missing tmux runtime to events

**Files:**
- Modify: `.worktrees/m1-tmux-runtime/src/application/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/src/runtime/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/src/transport/http/external.rs` if an explicit observe endpoint is needed; prefer not adding External API unless unavoidable.
- Modify: `.worktrees/m1-tmux-runtime/tests/tmux_runtime_m1.rs`

- [ ] **Step 1: Write failing test**

Add `observe_missing_tmux_runtime_projects_session_error` that creates a session, externally kills the tmux session, calls an internal application service `RuntimeObservationService::observe_session`, then asserts session state `error` and event list contains `session.error` from `runtime_manager`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 observe_missing_tmux_runtime_projects_session_error -- --test-threads=1`
Expected: FAIL because no observation service exists.

- [ ] **Step 3: Implement minimal code**

Add `RuntimeObservationService` in `application/mod.rs` that:
- loads session and runtime binding;
- calls `RuntimeManager::is_alive(runtime_ref)`;
- if missing and session is non-terminal, ingests `session.error` with diagnostic message;
- does not expose tmux metadata via External API.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 observe_missing_tmux_runtime_projects_session_error -- --test-threads=1`
Expected: PASS.

## Task 5: active turn crash maps turn.failed

**Files:**
- Modify: `.worktrees/m1-tmux-runtime/src/application/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/tests/tmux_runtime_m1.rs`

- [ ] **Step 1: Write failing test**

Add `observe_missing_tmux_runtime_fails_active_turn` that creates a generic session, submits a turn so it is active/queued, externally kills tmux, observes runtime, then asserts session `error`, turn `failed`, and events contain both `session.error` and `turn.failed`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 observe_missing_tmux_runtime_fails_active_turn -- --test-threads=1`
Expected: FAIL because observation does not fail active turn yet.

- [ ] **Step 3: Implement minimal code**

Extend observation service to check `session.current_turn_id`. If present, ingest `turn.failed` with concise runtime crash failure payload before/after `session.error` in a reducer-safe order.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 observe_missing_tmux_runtime_fails_active_turn -- --test-threads=1`
Expected: PASS.

## Task 6: pi sessions use tmux runtime, M0 behavior does not regress

**Files:**
- Modify: `.worktrees/m1-tmux-runtime/src/runtime/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/src/application/mod.rs`
- Modify: `.worktrees/m1-tmux-runtime/tests/tmux_runtime_m1.rs`
- Existing test: `.worktrees/m1-tmux-runtime/tests/pi_adapter_m0.rs`

- [ ] **Step 1: Write failing test**

Add `create_pi_session_creates_real_tmux_runtime` that creates `client_type = "pi"`, asserts binding backend `tmux`, asserts capabilities remain pi capabilities, and asserts the tmux session exists.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd .worktrees/m1-tmux-runtime && cargo test --test tmux_runtime_m1 create_pi_session_creates_real_tmux_runtime -- --test-threads=1`
Expected: FAIL until pi start path uses tmux runtime.

- [ ] **Step 3: Implement minimal code**

Ensure `RuntimeManager::start_session` uses tmux for all supported client types and only varies capabilities/client command. Keep M0 fake pi tests working while runtime lifecycle comes from tmux.

- [ ] **Step 4: Run test and M0 pi tests**

Run:
```bash
cd .worktrees/m1-tmux-runtime && \
cargo test --test tmux_runtime_m1 create_pi_session_creates_real_tmux_runtime -- --test-threads=1 && \
cargo test --test pi_adapter_m0 -- --test-threads=1
```
Expected: PASS.

## Task 7: docs and milestone verification

**Files:**
- Modify: `.worktrees/m1-tmux-runtime/README.md`
- Modify: `.worktrees/m1-tmux-runtime/MILESTONE.md`

- [ ] **Step 1: Update docs**

Document that M1 requires real `tmux`, runtime tests use real tmux, and run command is:

```bash
cargo test --test tmux_runtime_m1 -- --test-threads=1
```

- [ ] **Step 2: Mark M1 complete**

Update `MILESTONE.md` M1 checklist only after verification passes.

- [ ] **Step 3: Full verification**

Run:
```bash
cd .worktrees/m1-tmux-runtime && \
cargo fmt --check && \
cargo test -- --test-threads=1 && \
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.
