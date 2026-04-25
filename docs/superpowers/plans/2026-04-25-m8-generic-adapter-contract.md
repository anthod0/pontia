# M8 Generic Adapter Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Validate that the Control Plane can complete MVP orchestration through a generic adapter contract without binding to pi, Claude Code, Codex, or any client-specific fields.

**Architecture:** Add a small generic adapter contract module that models capabilities, accepted turn input, event reporting, and artifact source registration. Wire the existing generic runtime manager to this contract with an in-memory/test-level adapter substitute and keep all externally visible state driven by Internal Event API/projections and artifact index rows.

**Tech Stack:** Rust, Tokio, Axum, SQLx/SQLite, Serde, existing integration test harness.

---

## File Structure

- Modify `src/adapters/mod.rs`: define the generic adapter contract types and a minimal in-memory `GenericTestAdapter` substitute.
- Modify `src/runtime/mod.rs`: use the adapter capability model and add opt-in constructor helpers for capability combinations used by tests.
- Modify `src/application/mod.rs`: add artifact source registration service method and keep turn submission identity/control-plane semantics intact.
- Create `tests/milestone8_generic_adapter_contract.rs`: integration tests for capability declaration, turn input handoff, Internal Event API event return flow, artifact registration/content read, and capability degradation.
- Modify `MILESTONE.md`: mark M8 complete after tests pass.
- Modify `README.md`: update current milestone and generic adapter contract notes.

## Task 1: Contract types and capability declaration

**Files:**
- Modify: `src/adapters/mod.rs`
- Modify: `src/runtime/mod.rs`
- Test: `tests/milestone8_generic_adapter_contract.rs`

- [ ] **Step 1: Write failing tests**
  - Add tests asserting a generic session exposes all seven M8 capabilities by name/field: `accept_task`, `report_turn_started`, `report_turn_finished`, `interrupt`, `stream_output`, `heartbeat`, `artifact_sources`.
  - Add a test that default generic runtime has explicit `interrupt: false`, `stream_output: false`, `heartbeat: false`, `artifact_sources: false` degradation.
- [ ] **Step 2: Run RED**
  - Run: `cargo test --test milestone8_generic_adapter_contract capability -- --nocapture`
  - Expected: compile/test failure because M8 capability fields/types are missing.
- [ ] **Step 3: Implement minimal contract**
  - Add `AdapterCapabilities` in `src/adapters/mod.rs`.
  - Extend/reuse `SessionCapabilities` with `report_turn_started` and `report_turn_finished`.
  - Map adapter capabilities into runtime binding metadata.
- [ ] **Step 4: Run GREEN**
  - Run: `cargo test --test milestone8_generic_adapter_contract capability -- --nocapture`
  - Expected: PASS.

## Task 2: Turn input handoff and Control Plane identity

**Files:**
- Modify: `src/adapters/mod.rs`
- Modify: `src/runtime/mod.rs`
- Test: `tests/milestone8_generic_adapter_contract.rs`

- [ ] **Step 1: Write failing test**
  - Submit a turn through External API and assert the accepted adapter input contains the Control Plane assigned `session_id`, Control Plane assigned `turn_id`, and user input.
  - Assert the request body cannot supply/override `turn_id`.
- [ ] **Step 2: Run RED**
  - Run: `cargo test --test milestone8_generic_adapter_contract turn_input -- --nocapture`
  - Expected: failure because accepted adapter input is not inspectable.
- [ ] **Step 3: Implement minimal handoff record**
  - Add an adapter test sink that records accepted `AgentInput` in a process-local test-safe store or through existing DB metadata.
  - Keep runtime submission independent from Axum.
- [ ] **Step 4: Run GREEN**
  - Run: `cargo test --test milestone8_generic_adapter_contract turn_input -- --nocapture`
  - Expected: PASS.

## Task 3: Event source return path through Internal Event API

**Files:**
- Test: `tests/milestone8_generic_adapter_contract.rs`

- [ ] **Step 1: Write failing/behavior test**
  - Create session and submit turn via External API.
  - Post `turn.started`, `turn.output`, and `turn.completed` with `source: agent_adapter` to `/internal/v1/events`.
  - Query External API turn and events; assert state is `completed`, output summary is populated, event source is `agent_adapter`, and no client-specific raw fields are required.
- [ ] **Step 2: Run RED/GREEN**
  - Run: `cargo test --test milestone8_generic_adapter_contract event_source -- --nocapture`
  - Expected may already PASS if existing M2/M5 behavior satisfies it; if it fails, implement only the missing behavior.

## Task 4: Artifact source provider registration and content read

**Files:**
- Modify: `src/adapters/mod.rs`
- Modify: `src/application/mod.rs`
- Test: `tests/milestone8_generic_adapter_contract.rs`

- [ ] **Step 1: Write failing test**
  - Register a file artifact source for a completed turn through a generic adapter/provider helper.
  - Query artifact metadata and content through External API.
  - Assert metadata hides `source_ref`, content matches the registered file, and turn output can reference the artifact id.
- [ ] **Step 2: Run RED**
  - Run: `cargo test --test milestone8_generic_adapter_contract artifact_source -- --nocapture`
  - Expected: failure because registration helper/service is missing.
- [ ] **Step 3: Implement minimal registration service**
  - Add a focused artifact registration function that inserts into `artifacts` with `source_ref` preserved internally and public metadata sanitized by existing query code.
- [ ] **Step 4: Run GREEN**
  - Run: `cargo test --test milestone8_generic_adapter_contract artifact_source -- --nocapture`
  - Expected: PASS.

## Task 5: Capability-specific degradation

**Files:**
- Test: `tests/milestone8_generic_adapter_contract.rs`

- [ ] **Step 1: Write behavior tests**
  - Confirm `interrupt` remains independently disabled for default generic runtime and External API returns `capability_unavailable` without creating `turn.interrupt_requested`.
  - Confirm artifact listing/content behavior remains available only for registered sources; empty source support returns empty list.
- [ ] **Step 2: Run tests**
  - Run: `cargo test --test milestone8_generic_adapter_contract degradation -- --nocapture`
  - Expected: PASS or targeted failure; implement only missing behavior.

## Task 6: Documentation and milestone completion

**Files:**
- Modify: `MILESTONE.md`
- Modify: `README.md`

- [ ] **Step 1: Run full verification**
  - Run: `cargo test`
  - Run: `cargo fmt --check`
  - Run: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] **Step 2: Update docs**
  - Mark M8 complete with validation commands.
  - Update README current implementation summary from M7/M6 to M8.
- [ ] **Step 3: Final verification**
  - Re-run the same verification commands.
