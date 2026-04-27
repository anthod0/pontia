# M0 pi Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Milestone 0 by adding a `client_type = "pi"` adapter path that exercises a real subprocess JSONL boundary with fake pi in tests and documents real pi local validation.

**Architecture:** Keep domain and External API semantics unchanged. Add a minimal pi adapter/runtime boundary that can run an executable speaking pi RPC-style JSONL over stdin/stdout; production default points to `pi --mode rpc`, while tests inject a fake executable. Runtime emits unified domain events and registers a transcript artifact through existing services.

**Tech Stack:** Rust 2024, Axum, SQLx/SQLite, Tokio subprocess/io, serde_json, existing artifact/event services, integration tests with temporary shell fake.

---

## File Structure

- Modify `src/config.rs`: add optional `LLMPARTY_PI_COMMAND` / `LLMPARTY_PI_ARGS` if needed for local real-pi command docs.
- Modify `src/adapters/mod.rs`: add pi capability defaults and RPC event parsing helpers.
- Modify `src/runtime/mod.rs`: route generic vs pi runtime starts/submissions; run pi subprocess JSONL in M0 synchronous/minimal fashion for submitted turns; register output artifact.
- Modify `src/application/mod.rs`: allow `client_type = "pi"`, pass DB pool into runtime manager or provide services needed for pi event/artifact writes.
- Add `tests/pi_adapter_m0.rs`: end-to-end fake-pi subprocess tests.
- Modify `README.md`: document M0 fake test strategy and optional real-pi local validation.
- Modify `MILESTONE.md`: mark M0 complete only after verification.

## Task 1: pi capability and client_type acceptance

**Files:**
- Modify: `.worktrees/m0-pi-adapter/src/adapters/mod.rs`
- Modify: `.worktrees/m0-pi-adapter/src/runtime/mod.rs`
- Modify: `.worktrees/m0-pi-adapter/src/application/mod.rs`
- Test: `.worktrees/m0-pi-adapter/tests/pi_adapter_m0.rs`

- [ ] **Step 1: Write failing test**

Add a test that creates `client_type = "pi"` session and asserts CREATED, session `client_type` is `pi`, `state` is `idle`, capabilities expose `accept_task/report_turn_started/report_turn_finished/stream_output/artifact_sources = true`, and `interrupt/heartbeat = false`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd .worktrees/m0-pi-adapter && cargo test --test pi_adapter_m0 pi_session_creation_exposes_m0_capabilities -q`
Expected: FAIL because test file or pi support does not exist.

- [ ] **Step 3: Implement minimal code**

Add pi capabilities and allow session creation for `generic` or `pi`. Runtime binding for pi should have `runtime_kind = "pi"`, `runtime_ref = "pi:<session_id>"`, and pi capabilities metadata.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd .worktrees/m0-pi-adapter && cargo test --test pi_adapter_m0 pi_session_creation_exposes_m0_capabilities -q`
Expected: PASS.

## Task 2: fake pi subprocess turn execution and event mapping

**Files:**
- Modify: `.worktrees/m0-pi-adapter/src/adapters/mod.rs`
- Modify: `.worktrees/m0-pi-adapter/src/runtime/mod.rs`
- Modify: `.worktrees/m0-pi-adapter/src/application/mod.rs`
- Test: `.worktrees/m0-pi-adapter/tests/pi_adapter_m0.rs`

- [ ] **Step 1: Write failing test**

Add a fake executable in a tempdir that reads one JSONL command from stdin and writes pi RPC-like JSONL events: `agent_start`, `message_update` with `text_delta`, and `agent_end`. Set env var to point runtime at this fake. Submit a turn to a pi session and assert the turn eventually is `completed`, events include `turn.started`, `turn.output`, `turn.completed` with source `agent_adapter`, and generic recorded inputs remain empty.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd .worktrees/m0-pi-adapter && cargo test --test pi_adapter_m0 pi_turn_runs_through_fake_rpc_and_projects_completed_state -q`
Expected: FAIL because submit still only records generic input / no pi subprocess event mapping.

- [ ] **Step 3: Implement minimal code**

For pi sessions, on submit, create `turn.created` and `turn.queued`, then invoke pi RPC fake via configured command, send `{"type":"prompt","message": input}` JSONL, read stdout lines to completion, map first `agent_start` to `turn.started`, `message_update.assistantMessageEvent.text_delta` to accumulated `turn.output`, and `agent_end` to `turn.completed`. Do not expose pi-specific payload fields.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd .worktrees/m0-pi-adapter && cargo test --test pi_adapter_m0 pi_turn_runs_through_fake_rpc_and_projects_completed_state -q`
Expected: PASS.

## Task 3: pi artifact registration and failed turn degradation

**Files:**
- Modify: `.worktrees/m0-pi-adapter/src/runtime/mod.rs`
- Modify: `.worktrees/m0-pi-adapter/src/application/mod.rs`
- Test: `.worktrees/m0-pi-adapter/tests/pi_adapter_m0.rs`

- [ ] **Step 1: Write failing tests**

Add tests for: (1) completed fake pi turn registers a readable transcript/output artifact visible via External API metadata and content; (2) fake pi process exits non-zero or emits malformed events and projects `turn.failed`, not completed.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd .worktrees/m0-pi-adapter && cargo test --test pi_adapter_m0 pi_artifact_is_registered_and_readable pi_rpc_failure_projects_turn_failed -q`
Expected: FAIL because artifact/failure handling missing.

- [ ] **Step 3: Implement minimal code**

Write accumulated output to a temp/workspace artifact file, register it with `ArtifactRegistrationService`, include artifact id in `turn.completed` payload. On subprocess spawn/read/exit failure, ingest `turn.failed` with a concise message. Keep unsupported interrupt behavior as `capability_unavailable`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd .worktrees/m0-pi-adapter && cargo test --test pi_adapter_m0 pi_artifact_is_registered_and_readable pi_rpc_failure_projects_turn_failed -q`
Expected: PASS.

## Task 4: docs, milestone update, full verification

**Files:**
- Modify: `.worktrees/m0-pi-adapter/README.md`
- Modify: `.worktrees/m0-pi-adapter/MILESTONE.md`

- [ ] **Step 1: Document validation**

Update README with fake-pi automated testing explanation and optional real `pi --mode rpc` local validation using env vars. Mark M0 complete in `MILESTONE.md` only if all verification passes.

- [ ] **Step 2: Run formatting and tests**

Run:
```bash
cd .worktrees/m0-pi-adapter && cargo fmt --check && cargo test && cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.
