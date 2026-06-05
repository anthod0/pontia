# Agent Graph 模型迁移实施计划

> **给 agentic workers：** 必须使用子技能：建议使用 `superpowers:subagent-driven-development`，或使用 `superpowers:executing-plans`，按任务逐项执行本计划。步骤使用 checkbox（`- [ ]`）语法用于跟踪。

**目标：** 破坏性地把 pilotfy 当前 SQLite-authoritative DAG 模型迁移到目标 Agent WorkItem graph 模型，同时只保留当前已经实现的 DAG 执行能力。

**架构：** WorkItem DAG 结构迁移到 graph-store 边界后面，并由 task events 投影生成；SQLite 只继续作为 operational runtime state 的权威来源，例如 WorkItemRun、ready/running/blocked projection、session、turn、idempotency。Scheduler 和 External API 聚合 graph 结构与 SQLite runtime projection，但 scheduler claim/retry/run mutation 仍然只在 SQLite 中完成。

**技术栈：** Rust 2024、Axum、SQLx/SQLite、可选 `lbug` feature、Tokio、serde/serde_json、现有 migrations 和 integration tests。

---

## 需求来源

开始实现前必须阅读：

- 目标设计：`specs/2026-05-06-agent-graph-model-design.md`
- 项目规则：`AGENTS.md`
- 当前 DAG model：`src/application/dag_models.rs`
- 当前 DAG service：`src/application/dag.rs`
- 当前 scheduler：`src/application/dag_scheduler.rs`
- 当前 query 聚合：`src/application/queries.rs`
- 当前 graph placeholder：`src/application/graph/`
- 当前 DAG migration：`migrations/0009_dag_runtime.sql`

## 非目标

本次迁移不要实现：

- WorkProduct 行为或 runtime artifact materialization。
- Human approval gate 状态机。
- ReviewPolicy trigger/materialization 行为。
- 通过 lbug 进行 scheduler claim/retry。
- 除 DAG 相关 `task_events` projection 外的完整 task replay framework。
- 兼容旧的 `work_items` / `work_item_edges` authoritative SQL tables。

## 目标结束状态

完成本计划后：

1. WorkItem DAG 结构只能通过 graph store API 访问。
2. SQLite `work_item_runtime_projection` 只包含 operational scheduler state。
3. SQLite `work_item_runs` 继续作为具体执行尝试的权威来源。
4. DAG mutation 会 append `task_events` 并 project 到 graph store。
5. Scheduler 从 graph store 读取 WorkItem 定义和依赖，从 SQLite claim runtime rows。
6. External API 通过 graph + SQLite runtime data 聚合返回原有 `TaskDagView` 形状。
7. Tests 不再直接查询旧 `work_items` / `work_item_edges`。

## 当前重要状态

当前实现把 SQLite 表当成 authoritative DAG：

- `work_items`
- `work_item_edges`
- `work_item_runtime_projection`
- `work_item_runs`
- `dag_proposals`
- `dag_signals`

重要现有代码路径：

- `DagService::apply_initial_dag` 插入 `work_items` 和 `work_item_edges`。
- `DagService::apply_patch` 修改 `work_items` 和 `work_item_runtime_projection`。
- `DagSchedulerService` join `work_items`、`work_item_edges` 和 `work_item_runtime_projection`。
- `ExternalQueryService::{get_task_dag,list_work_items,list_work_item_edges}` 读取 SQL DAG tables。
- `GraphProjectionService` 当前基本是 no-op。

本计划有意打破上述模型。

## 合并后的任务结构

原计划拆成 9 个较细任务。当前版本合并为 5 个较大的 agent 任务：

1. Graph store 基础设施：types、schema、SQLite store、event projection。
2. DAG mutation 迁移：`DagService` apply/patch 改为 event + graph + runtime projection。
3. Scheduler 迁移：graph 读结构，SQLite claim runtime。
4. Query/API/Agent tools/Signals 迁移：所有外部 view 和 agent context 改为 graph + runtime 聚合，并补齐 signal event projection。
5. 清理与全量验证：移除旧表假设、更新 tests/support、运行全套验证。

每个任务仍应由一个全新的 agent 顺序执行。任务粒度变大后，每个 agent 需要在任务内自行保持小步提交和测试优先。

---

## Task 1：建立 Graph Store 基础设施

**起始状态：** 当前 `src/application/graph/types.rs` 只有 provenance placeholder types；`GraphProjectionService` 基本 no-op；migration 创建旧 `work_items` / `work_item_edges` authoritative tables。

**目标：** 一次性建立新的 graph model 类型、SQLite-backed graph store、graph schema、以及从 `task_events` 投影 graph 的能力。此任务结束后，graph store 能独立 persist/query WorkItem DAG，但业务代码可以暂时仍未完全切换。

**文件：**

- 修改：`src/application/graph/types.rs`
- 新建：`src/application/graph/store.rs`
- 新建：`src/application/graph/sqlite_store.rs`
- 修改：`src/application/graph/service.rs`
- 修改：`src/application/graph/mod.rs`
- 修改：`src/application/mod.rs`
- 修改：`migrations/0009_dag_runtime.sql` 或新建 destructive migration
- 新建：`tests/graph_projection.rs`

### 需要实现的内容

- [ ] **Step 1：定义 graph-facing types**

在 `src/application/graph/types.rs` 中保留仍被引用的 `GraphRuntimeConfig` 和 provenance structs，同时新增：

- `TaskNode`
- `WorkItemNode`
- `WorkItemEdgeRecord`
- `SignalNode`
- `GraphEdgeKind`
- `TaskGraphSnapshot`

`WorkItemNode` 至少包含：

```rust
pub struct WorkItemNode {
    pub work_item_id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub action: String,
    pub execution_profile_id: String,
    pub execution_profile_version: Option<String>,
    pub review_policy: Option<Value>,
    pub execution_policy: Option<Value>,
    pub escalation_policy: Option<Value>,
    pub priority: i64,
    pub optional: bool,
    pub parallelizable: bool,
    pub acceptance_criteria: Value,
    pub active: bool,
    pub ref_: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}
```

- [ ] **Step 2：建立 graph store API**

创建 `src/application/graph/store.rs`，定义 request types 和 store 操作。可以先使用具体 `SqliteDagGraphStore` inherent methods；如果使用 trait 导致 async trait 复杂，不要为此阻塞。

需要支持：

- `upsert_task`
- `upsert_work_item`
- `set_work_item_active`
- `add_edge`
- `upsert_signal`
- `task_graph`
- `get_work_item`
- `list_dependencies`

- [ ] **Step 3：调整 schema**

因为不需要兼容旧数据，优先重写/squash `0009_dag_runtime.sql`。如果团队希望保留 migration history，则新建 `0016_graph_model_destructive_migration.sql`。

创建 graph-store tables：

```sql
CREATE TABLE graph_tasks (...);
CREATE TABLE graph_work_items (...);
CREATE TABLE graph_work_item_edges (...);
CREATE TABLE graph_signals (...);
```

保留或重建 runtime tables：

- `work_item_runs`
- `work_item_runtime_projection`
- `dag_proposals`
- `dag_signals`

要求：

- `work_item_runtime_projection.work_item_id` 不再 FK 到旧 `work_items`。
- `work_item_runs.work_item_id` 不再 FK 到旧 `work_items`。
- 添加 runtime lease 字段：`lease_owner TEXT`、`lease_expires_at TEXT`。
- 新 graph edge type 至少支持：`depends_on`、`reviews`、`supersedes`、`caused_by`。

- [ ] **Step 4：实现 `SqliteDagGraphStore`**

在 `src/application/graph/sqlite_store.rs` 实现 graph table 的读写。JSON 字段以 string 存储，读取时 parse 回 `serde_json::Value`。

实现必须 idempotent：

- `upsert_*` 使用 `INSERT ... ON CONFLICT DO UPDATE`。
- `add_edge` 对重复边不报错或使用 unique conflict no-op。

- [ ] **Step 5：实现 `GraphProjectionService`**

`GraphProjectionService` 至少支持从 `task_events` project：

- `task.created`
- `dag.applied`
- `dag.patch_applied`
- `work_item.created`
- `work_item.edge_added`
- `work_item.superseded`
- `signal.emitted`

Projection 规则：

- `work_item.created` upsert `graph_work_items`。
- `work_item.edge_added` insert graph edge。
- `work_item.superseded` 设置旧 WorkItem `active=false`；如果 payload 有 replacement id，则添加 `supersedes` edge。
- `signal.emitted` upsert `graph_signals`。

- [ ] **Step 6：写并运行 graph projection tests**

创建 `tests/graph_projection.rs`，至少覆盖：

```rust
#[tokio::test]
async fn sqlite_graph_store_persists_work_items_and_edges() { ... }

#[tokio::test]
async fn graph_projection_rebuilds_from_task_events() { ... }
```

运行：

```bash
cargo test --test graph_projection
cargo check
```

预期：PASS。

- [ ] **Step 7：提交**

```bash
git add src/application/graph src/application/mod.rs migrations tests/graph_projection.rs
git commit -m "refactor: introduce DAG graph store infrastructure"
```

**结束状态：** Graph store、schema、projection 都已存在并可独立测试。旧业务代码可能仍引用旧表，后续任务继续切换。

---

## Task 2：迁移 DagService Mutation 到 Event + Graph + Runtime Projection

**起始状态：** Graph store 可用，但 `DagService::apply_initial_dag` / `apply_patch` 仍可能直接写旧 SQL DAG tables。

**目标：** `DagService` 成为 proposal validation、task event append、graph projection、runtime projection initialization 的 orchestrator。此任务结束后，DAG 创建和 patch 不再写旧 `work_items` / `work_item_edges`。

**文件：**

- 修改：`src/application/dag.rs`
- 视需要修改：`src/application/dag_models.rs`
- 视需要修改：`src/application/dag_validator.rs`
- 修改测试：`tests/dag_runtime.rs`
- 修改测试：`tests/agent_tools_submit_plan_api.rs`
- 可能修改：`tests/dag_planner_flow.rs`

### 需要实现的内容

- [ ] **Step 1：先更新测试断言**

把测试中对旧表的断言：

```sql
SELECT COUNT(*) FROM work_items
SELECT COUNT(*) FROM work_item_edges
```

替换成：

- graph store 查询，或
- External API DAG view，或
- runtime-only SQL 查询。

必须覆盖：

- Initial DAG apply 创建 graph WorkItems。
- Runtime projection 为每个 active WorkItem 初始化 row。
- Dependency edge 会阻塞 downstream WorkItem 直到 upstream completed。
- Patch add_work_item 创建 graph WorkItem 和 runtime row。
- Patch add_edge 创建 graph edge。
- Patch supersede 设置 graph WorkItem inactive，并把 runtime state 标记为 `superseded`。

- [ ] **Step 2：重写 `apply_initial_dag`**

逻辑改为：

1. Validate plan shape。
2. Validate execution profiles。
3. 如果 graph store 已有 active WorkItems，拒绝。
4. 为 draft WorkItems 生成 authoritative WorkItem IDs。
5. Append task events：
   - `dag.applied`
   - 每个 WorkItem 一个 `work_item.created`
   - 每条 edge 一个 `work_item.edge_added`
6. Project events 到 graph store。
7. 基于 graph snapshot 初始化 `work_item_runtime_projection`。

不要再 insert 旧 `work_items` 或 `work_item_edges`。

- [ ] **Step 3：重写 `apply_patch`**

逻辑改为：

1. 基于 graph snapshot validate patch。
2. 为新增 WorkItems 生成 IDs。
3. Append task events：
   - `dag.patch_applied`
   - `work_item.created`
   - `work_item.edge_added`
   - `work_item.superseded`
4. Project events 到 graph store。
5. 为新增 active WorkItems 插入 runtime projection rows。
6. 把 superseded WorkItems 的 runtime state 标记为 `superseded`。

- [ ] **Step 4：把 runtime projection 初始化改为基于 graph snapshot**

替换旧 SQL join edge tables 的 `initialize_projection` 逻辑：

1. 加载 graph snapshot。
2. 对每个缺少 runtime row 的 active WorkItem：
   - 如果存在 active `depends_on` upstream 且其 runtime state 不是 `completed` / `replan_anchor`：state = `blocked`。
   - 否则 state = `ready`。
3. 插入 runtime row，并从 graph WorkItem 复制 priority/optional/parallelizable。

- [ ] **Step 5：更新 patch validation**

`validate_patch` 不再查询旧 `work_items` / `work_item_edges`。它应基于 graph snapshot：

- 验证 referenced WorkItem 存在且 active。
- 验证 supersede target 不在 running。
- 验证新增 edges 不会造成 depends_on cycle。
- 仍然只允许当前已实现 patch operations，不扩展 WorkProduct/human approval/review behavior。

- [ ] **Step 6：运行测试**

运行：

```bash
cargo test --test dag_runtime --test agent_tools_submit_plan_api --test dag_planner_flow
```

预期：PASS。

- [ ] **Step 7：提交**

```bash
git add src/application/dag.rs src/application/dag_models.rs src/application/dag_validator.rs tests/dag_runtime.rs tests/agent_tools_submit_plan_api.rs tests/dag_planner_flow.rs
git commit -m "refactor: apply DAG mutations through graph projection"
```

**结束状态：** DAG apply/patch 已迁移到 event + graph + runtime projection，旧 SQL DAG tables 不再作为 mutation target。

---

## Task 3：迁移 Scheduler 到 Graph 结构 + SQLite Runtime Claim

**起始状态：** DAG mutation 已写入 graph store。Scheduler 仍可能 join 旧 SQL DAG tables。

**目标：** Scheduler 使用 graph store 获取 WorkItem 定义和依赖，同时 runtime claim/run/session/turn mutation 仍保留在 SQLite。此任务结束后，scheduler 不再读取旧 `work_items` / `work_item_edges`。

**文件：**

- 修改：`src/application/dag_scheduler.rs`
- 如 `SchedulerWorkItem` 变化，修改：`src/application/prompt_rendering.rs`
- 修改测试：`tests/dag_scheduler.rs`
- 可能修改：`tests/dag_run_result.rs`

### 需要实现的内容

- [ ] **Step 1：先更新 scheduler tests**

移除旧 `work_items` 直接 SQL reads。改用：

- External API DAG view，或
- `SqliteDagGraphStore::task_graph`，或
- 仅对 runtime state 使用 `work_item_runtime_projection` SQL。

Tests 仍必须覆盖：

- Ready item dispatch。
- Blocked downstream 不 dispatch。
- Open blocking signal 暂停 scheduler。
- Replan anchor 语义。
- Priority ordering。

- [ ] **Step 2：重写 `recompute_ready`**

用 graph snapshot 替换基于 `work_item_edges` 的 SQL `EXISTS` checks：

1. 一次性加载 graph snapshot。
2. 从 SQLite 加载 task runtime states。
3. 对每个 runtime row in `pending|ready|blocked`，基于 graph `depends_on` edges 计算依赖是否完成。
4. Batch update runtime rows 到 `ready` 或 `blocked`。

- [ ] **Step 3：重写 `claim_ready_work_item`**

Claim 仍必须是 SQLite atomic：

1. 从 `work_item_runtime_projection WHERE current_state='ready'` 按 priority/ready_at 选一个 candidate `work_item_id`。
2. 从 graph store 加载 WorkItem。
3. 验证 active 且 profile 存在。
4. 使用 graph + runtime state 重新检查 dependencies。
5. 用 conditional `WHERE current_state='ready'` 把 runtime row update 为 `running`。

如果 graph item missing/inactive，把 projection 标记为 `blocked`，reason = `missing_or_inactive_graph_work_item`，然后继续。

- [ ] **Step 4：调整 `SchedulerWorkItem` 数据来源**

保留 `SchedulerWorkItem` 作为 scheduler-local struct，但从 `WorkItemNode` + runtime row 填充。确保 prompt rendering 仍能拿到：

- title
- description
- kind
- action
- execution_profile_id/version
- current_attempt

- [ ] **Step 5：确认 run/session/turn mutation 不迁移到 graph**

`create_work_item_run`、`find_or_create_session`、`dispatch_run` 仍然写 SQLite/session/turn 体系。不要引入 graph claim 或 graph run state。

- [ ] **Step 6：运行测试**

运行：

```bash
cargo test --test dag_scheduler --test dag_run_result
```

预期：PASS。

- [ ] **Step 7：提交**

```bash
git add src/application/dag_scheduler.rs src/application/prompt_rendering.rs tests/dag_scheduler.rs tests/dag_run_result.rs
git commit -m "refactor: schedule DAG work from graph store"
```

**结束状态：** Scheduler 不再读取旧 SQL WorkItem/edge tables，claim/retry/run 仍由 SQLite runtime projection 管理。

---

## Task 4：迁移 Query/API/Agent Tools/Signals 到 Graph-Aware 模型

**起始状态：** DAG mutation 和 scheduler 已使用 graph store。External query layer、agent tools、signal side effects 可能仍读取或写入旧 DAG 假设。

**目标：** External API 和 agent tools 通过 graph 结构 + SQLite runtime/run/signals 聚合返回视图；signals 在保留当前 operational behavior 的同时 append `signal.emitted` 并 project 到 graph。

**文件：**

- 修改：`src/application/queries.rs`
- 修改：`src/application/mapping.rs`
- 修改：`src/application/agent_tools/mod.rs`
- 如 view shape 变化，修改：`src/application/agent_tools/rendering.rs`
- 修改：`src/application/dag_run_result.rs`
- 修改：`src/application/tasks/commands.rs`
- 修改：`src/application/dag_planning.rs`
- 修改：`src/application/graph/service.rs`
- 修改测试：`tests/dag_external_api.rs`
- 修改测试：`tests/agent_tools_raise_signal_api.rs`
- 修改测试：`tests/dag_planner_flow.rs`
- 修改测试：`tests/dag_run_result.rs`

### 需要实现的内容

- [ ] **Step 1：重写 `ExternalQueryService::get_task_dag`**

实现形状：

1. `let graph = graph_store.task_graph(task_id).await?;`
2. 从 SQLite 按 task 加载 runtime projection rows。
3. 从 SQLite 按 task 加载 `work_item_runs`。
4. 从 SQLite 按 task 加载 `dag_signals`。
5. 把每个 `WorkItemNode` 转成 `WorkItemRecord`，并 attach optional `WorkItemRuntimeView`。
6. 把 graph edges 转成 `WorkItemEdgeView`。
7. 从 graph active WorkItems + runtime rows + signals 计算 summary。

- [ ] **Step 2：重写 list helpers**

更新：

- `list_work_items`
- `list_work_item_edges`
- `get_task_dag_summary`

这些方法应使用同一套 graph + runtime aggregation。可以创建 private helper 返回 graph snapshot + runtime map，避免重复太多逻辑。

- [ ] **Step 3：更新 agent tools context**

在 `AgentToolService::get_context` 中：

- Planning context 的 DAG 来自 graph-backed query。
- Execution context 的 current WorkItem 来自 graph-backed query。
- Dependency edges 来自 graph-backed `list_work_item_edges`。
- Upstream completed items 来自 graph-backed `list_work_items` + runtime state。

- [ ] **Step 4：迁移 signals 到 graph-aware events**

所有 signal creation paths 在保留 `dag_signals` operational row 的同时 append `task_events`：`signal.emitted`。

涉及路径：

- `DagRunResultService::raise_signal`
- submit result failure/needs_input paths 内部创建 signal 的位置
- `TaskCommandService::submit_human_signal` 或等价 human signal method

Payload 应包含：

```json
{
  "signal_id": "...",
  "task_id": "...",
  "work_item_id": "... optional ...",
  "run_id": "... optional ...",
  "source_session_id": "... optional ...",
  "source": "agent|human|system",
  "kind": "...",
  "summary": "...",
  "detail": "...",
  "severity": "low|medium|high",
  "related_refs": []
}
```

`GraphProjectionService` 处理 `signal.emitted`：upsert `SignalNode`，并在当前 graph tables 支持的情况下添加 task-to-signal relation。

- [ ] **Step 5：保留当前 signal blocking 行为**

不要改变当前 runtime side effects：

- needs_input 仍应像当前一样 block 或 mark run as needs_input。
- Replan signals 仍应被现有 replanning flow 获取。

- [ ] **Step 6：更新并运行测试**

测试应 assert API/view 行为，而不是实现表：

- `GET /external/v1/tasks/{task_id}/dag` 返回 work items、edges、runs、signals、summary。
- planning 的 `getContext` 包含来自 graph store 的当前 DAG。
- execution 的 `getContext` 包含当前 WorkItem、dependencies、upstream completed items。
- `raiseSignal` 仍记录 signal，并在需要时 block 当前 run。
- signal 同时产生 `task_events` 并可 project 到 graph。

运行：

```bash
cargo test --test dag_external_api --test agent_tools_raise_signal_api --test dag_planner_flow --test dag_run_result
```

预期：PASS。

- [ ] **Step 7：提交**

```bash
git add src/application/queries.rs src/application/mapping.rs src/application/agent_tools src/application/dag_run_result.rs src/application/tasks src/application/dag_planning.rs src/application/graph tests/dag_external_api.rs tests/agent_tools_raise_signal_api.rs tests/dag_planner_flow.rs tests/dag_run_result.rs
git commit -m "refactor: aggregate DAG views from graph and runtime projections"
```

**结束状态：** External API、agent tools、signals 都已切到 graph-aware 模型，不再依赖旧 SQL DAG tables。

---

## Task 5：移除旧模型假设并完成全量验证

**起始状态：** 主要业务路径已迁移到 graph store，但 tests/support、注释、migration 残留或 stale assumptions 可能仍存在。

**目标：** 清理旧 `work_items` / `work_item_edges` authoritative 假设，更新 test support，运行全套验证，确保后续开发可以基于新模型继续。

**文件：**

- 修改：`tests/support/agent_tools.rs`
- 修改：所有仍引用旧表的 tests
- 根据 compiler/clippy/test failures 修改必要文件
- 如需记录实现状态，更新：`specs/2026-05-06-agent-graph-model-design.md`
- 如 public behavior 或 setup 改变，更新：`README.md`

### 需要实现的内容

- [ ] **Step 1：搜索旧 references**

运行：

```bash
rg -n "\bwork_items\b|\bwork_item_edges\b" src tests migrations
```

预期：application code 不再引用旧 app-level tables。只有新 graph-store table names 如 `graph_work_items` 可以出现。

- [ ] **Step 2：替换 test helpers**

更新 `tests/support/agent_tools.rs` helpers，使其通过以下方式创建 graph WorkItems：

- `DagService::apply_initial_dag`
- graph store APIs
- task events + projection

只有测试明确需要 runtime setup 时，才可以直接插入 `work_item_runtime_projection` 或 `work_item_runs`。

- [ ] **Step 3：替换所有旧表断言**

把旧 DAG table 的直接 SQL 断言替换成：

- External API DAG view assertions，或
- `SqliteDagGraphStore::task_graph`，或
- runtime-only SQL assertions。

- [ ] **Step 4：运行 DAG 相关测试**

运行：

```bash
cargo test --test graph_projection --test dag_runtime --test dag_scheduler --test dag_external_api --test dag_planner_flow --test dag_run_result --test agent_tools_submit_plan_api --test agent_tools_raise_signal_api
```

预期：PASS。

- [ ] **Step 5：运行 formatting**

运行：

```bash
cargo fmt --check
```

预期：PASS。如果失败，运行 `cargo fmt`，然后重新运行 `cargo fmt --check`。

- [ ] **Step 6：运行完整 Rust tests**

运行：

```bash
cargo test
```

预期：PASS。

- [ ] **Step 7：运行 clippy**

运行：

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

预期：PASS。

- [ ] **Step 8：如果触碰 frontend/client，则运行对应测试**

如果本迁移修改了 dashboard 或 clients，运行相关命令：

```bash
pnpm --dir=apps/dashboard run check
pnpm --dir clients/pi test
pnpm --dir clients/pi typecheck
pnpm --dir clients/claude-code test
pnpm --dir clients/claude-code typecheck
```

预期：被触碰区域 PASS。不要使用 npm。

- [ ] **Step 9：最终搜索 stale assumptions**

运行：

```bash
rg -n "old DAG|work_items|work_item_edges|SQLite-authoritative|GraphProjectionService.*no-op" src tests specs README.md
```

预期：没有 stale application assumptions。Migration history 或本计划中的说明可以保留。

- [ ] **Step 10：提交 cleanup**

```bash
git add .
git commit -m "chore: verify graph model migration"
```

**结束状态：** 主线可以基于新的 graph-oriented model 继续后续 DAG 开发。

---

## 跨任务验证矩阵

| 行为 | 迁移后是否必须继续工作 | 建议测试文件 |
|---|---|---|
| Planner submit initial DAG | 是 | `tests/agent_tools_submit_plan_api.rs` |
| DAG task starts planning | 是 | `tests/dag_planner_flow.rs` |
| Scheduler dispatches ready WorkItem | 是 | `tests/dag_scheduler.rs` |
| Dependency blocks downstream | 是 | `tests/dag_scheduler.rs`, `tests/dag_runtime.rs` |
| submitResult completes run | 是 | `tests/dag_run_result.rs` |
| raiseSignal records and blocks | 是 | `tests/agent_tools_raise_signal_api.rs` |
| External DAG API returns view | 是 | `tests/dag_external_api.rs` |
| Old SQL DAG tables are not used | 是 | Task 5 的 `rg` checks |

## Agent 交接说明

- 把 `specs/2026-05-06-agent-graph-model-design.md` 当成目标语义的来源。
- 本迁移允许破坏性调整，不要花精力保留旧 dev data。
- 保留当前行为，不保留当前 storage implementation。
- 如果发现 lbug 集成成本过高，保留 `SqliteDagGraphStore` 作为 graph-store 实现。重要的是代码边界和数据归属，而不是物理数据库引擎。
- Scheduler 绝不能把 graph store 当成 atomic ready queue。SQLite runtime projection 仍是 operational claim surface。
- 本迁移不要添加 WorkProduct/human approval/review policy 行为。
