# Planner-assisted Dispatch and Kùzu Graph Seed Design

> Status: design draft for future implementation by agent sessions.
>
> Scope: introduce a planner-assisted dispatch loop and a shallow embedded Kùzu graph projection boundary. This document intentionally does **not** design the full multi-agent workflow graph, managed intermediate document artifacts, or graph-based scheduling.

## Goal

Move task creation from manual workspace confirmation toward planner-assisted dispatch:

```text
user task
  -> planner resolves workspace or asks for more input
  -> backend dispatches once workspace is resolved
```

At the same time, establish the architectural boundary that graph relationships belong in embedded Kùzu projections, not ad-hoc SQLite graph tables. Kùzu is introduced shallowly so later provenance/workflow graph work can grow without first building a temporary SQLite graph subsystem.

## Current Backend Context

The backend already has the task-first foundation:

- `tasks` is the user-facing lifecycle projection.
- `task_events` records task lifecycle history.
- `workspaces` stores canonical workspace records.
- `TaskCommandService::dispatch_task(...)` creates/selects session and submits turn.
- `POST /external/v1/tasks/{task_id}/confirm-workspace` manually resolves ambiguous tasks.
- Task creation without workspace currently enters `needs_confirmation` with `routing_state = ambiguous`.
- Task state syncs from turn lifecycle events.

This design changes the missing-workspace path. Instead of immediately requiring manual confirmation, the backend asks a planner to resolve the workspace. Manual confirmation remains the fallback.

## Non-goals

This design does not implement:

- Full workflow node/edge graph.
- Graph-based scheduler.
- Multi-step plan/write/execute/review lifecycle.
- Managed intermediate document artifacts.
- Long-lived planner memory.
- Kùzu as source of truth.
- Kùzu-backed dispatch decisions.
- Replacement of SQLite for current task/session/turn state.

## Key Decisions

### 1. Planner is a workspace-resolution loop, not the whole workflow engine

The planner's first responsibility is to get a task to a dispatchable state.

It may conclude:

```text
resolved     -> workspace is known; backend can dispatch
needs_input  -> planner needs user or helper-agent input before it can resolve
failed       -> planner cannot resolve; backend falls back to manual confirmation
```

`needs_input` is not a final task outcome. It is an intermediate planner state.

### 2. Planner proposes; backend owns state and dispatch

The planner must not directly mutate the database, create sessions, submit turns, or write files. It returns a structured decision. `TaskCommandService` or a focused orchestration helper applies that decision.

```text
Planner:      infer / ask / explain
Backend:      persist / transition / dispatch / recover
Runtime:      execute a specific turn once dispatched
```

### 3. Planner sessions are one-shot

The first planner implementation should use a one-shot `pi` runtime session for each planner attempt. The session is created for one analysis turn and terminated afterward.

Reason: reusable planner sessions risk context contamination between unrelated tasks.

### 4. Kùzu is introduced now, shallowly

Kùzu should be introduced now to establish that graph relationships are projected to a graph store. But it should not be deeply coupled to dispatch.

Initial role:

```text
SQLite/task_events = source of truth
Kùzu              = eventually-consistent graph projection
```

If Kùzu projection fails, task creation and dispatch should still succeed.

### 5. Intermediate artifacts are postponed

Planner output should initially be structured JSON stored in `task_events.payload`. Long planner reports, design docs, implementation plans, and review documents are deferred until a managed artifact design is agreed.

## Planner Lifecycle

### New planner states mapped onto task fields

No new top-level task state is strictly required for the first implementation. Existing fields can represent the planner lifecycle:

| Planner condition | `tasks.state` | `tasks.routing_state` | Meaning |
| --- | --- | --- | --- |
| planner running | `routing` | `pending` | backend is trying to resolve workspace |
| planner needs user/helper input | `needs_confirmation` or new `needs_input` | `ambiguous` | task is not dispatchable yet |
| planner resolved workspace | `queued`/`running` after dispatch | `matched` | dispatch succeeded or is underway |
| planner failed | `needs_confirmation` | `failed` | manual confirmation fallback |

Recommendation for first implementation: reuse `needs_confirmation` for planner `needs_input`, but set `routing_reason` and task event payload to clarify that the planner requested more information. A later migration can add a more precise `needs_input` state if WebUI needs it.

### Planner attempt flow

```text
POST /external/v1/tasks without workspace
  -> insert task: state=created, routing_state=pending
  -> task.created
  -> state=routing
  -> task.planning_started
  -> run one-shot PiTaskPlanner
  -> parse PlannerDecision

if status=resolved:
  -> task.planning_resolved
  -> dispatch_task(..., DispatchRoutingUpdate::Matched)

if status=needs_input:
  -> state=needs_confirmation, routing_state=ambiguous
  -> task.planning_needs_input
  -> return task to WebUI

if status=failed / timeout / invalid JSON:
  -> state=needs_confirmation, routing_state=failed
  -> task.planning_failed or task.routing_failed
  -> return task to WebUI for manual confirmation
```

### Planner resume flow

Add an endpoint for user-provided planner input:

```text
POST /external/v1/tasks/{task_id}/planner-input
```

Request:

```json
{
  "message": "This task is for the llmparty project.",
  "client_type": "pi"
}
```

Flow:

```text
load task
validate task is not terminal and has no turn_id
record task.planning_input_received
rerun one-shot PiTaskPlanner with original task input + prior planner decisions + user message
apply PlannerDecision as above
```

This endpoint is separate from `confirm-workspace`:

- `confirm-workspace` is direct human selection of workspace.
- `planner-input` gives the planner more information and lets it continue resolving.

## PlannerDecision Schema

The planner should return only JSON matching this shape.

```json
{
  "decision_id": "dec_optional_or_backend_generated",
  "status": "resolved | needs_input | failed",
  "workspace": {
    "workspace_id": "wks_optional",
    "canonical_path": "/absolute/path/or/null",
    "confidence": 0.91,
    "reason": "Why this workspace was selected"
  },
  "needs_input": {
    "question": "Which project should this task apply to?",
    "suggested_candidates": [
      {
        "workspace_id": "wks_...",
        "canonical_path": "/path/to/project",
        "reason": "Possible match"
      }
    ]
  },
  "reason": "Top-level explanation",
  "evidence": [
    {
      "evidence_id": "ev_optional_or_backend_generated",
      "kind": "workspace_candidate | recent_task | session_history | user_input | heuristic | other",
      "ref": "wks_... or task_... or free-form ref",
      "summary": "Evidence summary"
    }
  ]
}
```

Validation rules:

- `status` is required.
- `resolved` requires either a known `workspace_id` or a canonical absolute path that can be upserted.
- `needs_input` requires a non-empty `needs_input.question`.
- `failed` requires a non-empty `reason`.
- `confidence` should be clamped to `0.0..=1.0`.
- Unknown fields are allowed in payload storage but ignored by state transitions.
- Backend may generate missing `decision_id` and `evidence_id` values.

## Planner Prompt Contract

The Pi planner prompt should include:

- Task ID.
- User task input.
- Candidate workspaces.
- Workspace metadata available today.
- Recent task/session summaries if cheap to query.
- Any prior planner decisions for this task.
- Any user planner-input messages.
- Explicit instruction: do not execute the task; only resolve workspace or ask for necessary information.
- Explicit instruction: return only valid JSON.

The planner should be told that its final goal is to resolve a workspace so the backend can dispatch the task.

## Application Services

### New `TaskPlannerService`

Responsibilities:

- Build planner context.
- Invoke a `TaskPlanner` implementation.
- Validate and normalize `PlannerDecision`.
- Return a decision to the caller.

It does **not** mutate task state except through caller-owned orchestration methods, unless a very small helper method records planner-specific events under `TaskCommandService` control.

### New `TaskPlanner` trait/interface

Conceptual interface:

```rust
trait TaskPlanner {
    async fn plan(&self, input: PlannerInput) -> Result<PlannerDecision>;
}
```

First implementation:

```text
PiTaskPlanner
```

Future implementations:

- Rule-based planner for tests.
- Claude Code planner.
- Hybrid planner using embedding/search.
- Planner that delegates helper actions.

### `TaskCommandService` changes

`create_task` should route missing-workspace requests through planner-assisted dispatch:

```text
create task
if workspace present:
  dispatch_task(...)
else:
  run planner attempt
  apply planner decision
```

Add method:

```text
submit_planner_input(task_id, request, idempotency_key)
```

This method resumes the planner loop.

## HTTP API Changes

### Create task

Existing:

```text
POST /external/v1/tasks
```

Behavior change only when `workspace` is omitted:

- Before: immediately `needs_confirmation`.
- After: planner attempts resolution first.

### Submit planner input

New:

```text
POST /external/v1/tasks/{task_id}/planner-input
```

Response:

```json
{
  "data": {
    "task": {}
  },
  "error": null
}
```

Rules:

- Requires authentication.
- Supports `Idempotency-Key`.
- Rejects terminal tasks.
- Rejects tasks with existing `turn_id`.
- Allowed when task is waiting on planner/user input or routing failed/ambiguous without dispatch.

## Task Events

Add task event types:

```text
task.planning_started
task.planning_completed
task.planning_resolved
task.planning_needs_input
task.planning_input_received
task.planning_failed
```

Payloads should include enough information to project a graph later:

```json
{
  "decision_id": "dec_...",
  "planner_client_type": "pi",
  "planner_session_id": "sess_...",
  "planner_turn_id": "turn_...",
  "status": "needs_input",
  "reason": "...",
  "workspace": {},
  "evidence": []
}
```

## Kùzu Shallow Projection

### Purpose

Introduce Kùzu as the graph projection target without making graph projection part of the critical dispatch path.

### Storage location

Default:

```text
<llmparty_data_dir>/graph/kuzu
```

Tests should use a temporary data directory.

### Initial graph schema

Nodes:

```text
Task(task_id, state)
Workspace(workspace_id, canonical_path)
Session(session_id, client_type)
Turn(turn_id, state)
Decision(decision_id, status, reason, confidence, created_at)
Evidence(evidence_id, kind, ref, summary)
```

Relationships:

```text
(Task)-[:HAS_DECISION]->(Decision)
(Decision)-[:DEPENDS_ON]->(Evidence)
(Task)-[:ROUTED_TO]->(Workspace)
(Task)-[:DISPATCHED_TO]->(Session)
(Session)-[:HAS_TURN]->(Turn)
```

Do not add workflow nodes/edges yet.

### Projection source

Projection should be derived from SQLite facts:

- `tasks`
- `task_events`
- `sessions`
- `turns`
- `workspaces`

The first useful projection is planner-related task events.

### Projection failure semantics

Kùzu projection failure must not fail task creation, planner input, workspace confirmation, or dispatch.

Record projection failures as logs or warnings. A later implementation can add a durable projection checkpoint/outbox.

### Initial graph API

Optional for the first Kùzu session, but useful as an exit criterion:

```text
GET /external/v1/tasks/{task_id}/provenance
```

Response shape:

```json
{
  "data": {
    "nodes": [],
    "edges": []
  },
  "error": null
}
```

This endpoint should return graph projection data if available. It may return an empty graph if Kùzu is disabled or projection has not run.

## Configuration

Add configuration options:

```text
LLMPARTY_PLANNER_ENABLED=true|false
LLMPARTY_PLANNER_CLIENT_TYPE=pi
LLMPARTY_PLANNER_TIMEOUT_MS=30000
LLMPARTY_GRAPH_ENABLED=true|false
LLMPARTY_GRAPH_DB_DIR=<path>
```

Recommended defaults for early implementation:

```text
planner enabled: false in tests unless explicitly enabled
planner client: pi
graph enabled: false unless Kùzu dependency is compiled/configured
```

The planner should be easy to disable so existing task behavior can remain stable during rollout.

## Error Handling

Planner failures should degrade to manual confirmation:

| Failure | Task result |
| --- | --- |
| pi session start fails | `needs_confirmation`, `routing_state=failed` |
| planner turn dispatch fails | `needs_confirmation`, `routing_state=failed` |
| planner timeout | `needs_confirmation`, `routing_state=failed` |
| invalid JSON | `needs_confirmation`, `routing_state=failed` |
| resolved unknown/nonexistent path | upsert workspace if valid absolute path; otherwise failed fallback |
| resolved low confidence | implementation may treat as `needs_input` or `needs_confirmation` |

Do not mark the task terminal `failed` for planner resolution failures unless there is no recoverable manual path. Prefer recoverable manual confirmation.

## Testing Strategy

Use a fake/rule-based planner for most tests. Real pi planner tests should be isolated because they require tmux/pi runtime behavior.

Required backend tests:

1. Creating a task without workspace invokes planner and dispatches when planner resolves workspace.
2. Planner `needs_input` returns task waiting for user input and records `task.planning_needs_input`.
3. `POST /tasks/{task_id}/planner-input` resumes planner and dispatches when resolved.
4. Planner failure/invalid JSON falls back to manual confirmation, not terminal failure.
5. Idempotency works for task creation and planner input.
6. Kùzu projection can project a planner decision into Task/Decision/Evidence/Workspace nodes when graph is enabled.
7. Kùzu projection failure does not fail task creation or dispatch.

## Agent Session Implementation Plan

Recommended: **three agent sessions**. This is enough separation to reduce risk while avoiding over-fragmentation.

### Agent Session 1: Planner boundary and fake planner

Goal: implement planner-assisted dispatch behavior without real pi or Kùzu dependency.

Scope:

- Add planner data types.
- Add `TaskPlannerService` and test/fake planner seam.
- Modify missing-workspace `create_task` path to use planner when enabled.
- Add planner task events.
- Add `POST /external/v1/tasks/{task_id}/planner-input`.
- Keep existing manual confirmation fallback.

Exit criteria:

```bash
cargo test --test global_workspace_tasks -- --nocapture
cargo test --all
```

Suggested commit:

```bash
git commit -m "feat: add planner-assisted task dispatch"
```

### Agent Session 2: One-shot pi planner implementation

Goal: implement the real `PiTaskPlanner` using one-shot pi sessions.

Scope:

- Build planner prompt.
- Create one-shot pi session.
- Submit planner turn.
- Observe/read planner output according to existing pi adapter mechanisms.
- Parse JSON decision.
- Terminate planner session.
- Add timeout/failure fallback tests where feasible.

Exit criteria:

```bash
cargo test --test global_workspace_tasks planner -- --nocapture
cargo test --test pi_adapter_m15 -- --nocapture
cargo test --all
```

Suggested commit:

```bash
git commit -m "feat: resolve tasks with one-shot pi planner"
```

### Agent Session 3: Kùzu shallow graph projection

Goal: establish Kùzu as the graph projection target for planner/task provenance without making it critical path.

Scope:

- Add Kùzu dependency/configuration.
- Initialize embedded Kùzu database under llmparty data dir.
- Add `GraphProjectionService`.
- Project Task/Decision/Evidence/Workspace/Session/Turn basics.
- Optionally add `GET /external/v1/tasks/{task_id}/provenance`.
- Ensure projection failures do not break task APIs.

Exit criteria:

```bash
cargo test --test global_workspace_tasks -- --nocapture
cargo test --all
cd apps/web && pnpm build
```

Suggested commit:

```bash
git commit -m "feat: add kuzu task provenance projection"
```

## Open Questions

1. Should planner `needs_input` reuse `needs_confirmation`, or should a new `needs_input` task state be added?
2. How should the real pi planner output be collected reliably: adapter event log, runtime log parsing, or a dedicated planner output file?
3. Should graph projection be behind a Cargo feature if Kùzu adds build complexity?
4. What minimum confidence should allow automatic dispatch?
5. Should planner sessions be visible in normal session lists, or marked as system/internal and hidden by default?

## Future Extensions

- Managed control-plane artifacts for planner reports, design docs, implementation plans, and reviews.
- Explicit workflow graph with nodes/edges once task planning semantics are known.
- Kùzu projections for files, artifacts, failure paths, and cross-workflow analysis.
- Helper-agent planner actions such as `inspect_workspace`, `search_recent_tasks`, or `ask_user` as structured planner actions.
- Graph query APIs for upstream/downstream impact and agent failure pattern analysis.
