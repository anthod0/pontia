# M2 Artifact Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe, explicit workspace artifact discovery and stronger artifact content safeguards for Milestone 2.

**Architecture:** Implement a service-layer discovery boundary in `src/application/mod.rs` that scans only the canonical session workspace, upserts artifact rows, and enriches metadata without emitting domain events. Expose it through a new authenticated External API endpoint and keep content reads limited to registered file sources with size consistency and large-file rejection.

**Tech Stack:** Rust 2024, Axum, SQLx/SQLite, serde_json, Tokio tests.

---

### Task 1: Discovery endpoint and service

**Files:**
- Modify: `src/application/mod.rs`
- Modify: `src/transport/http/external.rs`
- Modify: `src/transport/http/mod.rs`
- Test: `tests/artifact_discovery_api.rs`

- [ ] Write failing tests for `POST /external/v1/sessions/{session_id}/artifacts/discover` discovering text files under workspace.
- [ ] Verify tests fail because route/service is missing.
- [ ] Implement `ArtifactDiscoveryService` and route handler.
- [ ] Run discovery tests and existing artifact tests.

### Task 2: Sandbox, symlink, metadata, kind, preview

**Files:**
- Modify: `src/application/mod.rs`
- Test: `tests/artifact_discovery_api.rs`

- [ ] Write failing tests for symlink/out-of-workspace rejection and metadata shape.
- [ ] Verify red.
- [ ] Implement canonical root checks, recursive scan, kind inference, preview generation, modified timestamp, checksum-like metadata, relative path.
- [ ] Run targeted tests.

### Task 3: Large file content guard

**Files:**
- Modify: `src/application/mod.rs`
- Test: `tests/artifact_content_api.rs`

- [ ] Write failing test proving large registered content is rejected with explicit error.
- [ ] Verify red.
- [ ] Add max content size guard before reading bytes.
- [ ] Run artifact content tests.

### Task 4: Docs and milestone update

**Files:**
- Modify: `README.md`
- Modify: `MILESTONE.md`

- [ ] Document discovery endpoint and M2 limitations.
- [ ] Mark M2 deliverables/acceptance complete where implemented.
- [ ] Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.
