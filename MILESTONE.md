# Agent Tools + DAG WebUI Validation Milestone

当前阶段目标是把已完成的 DAG-only 调度模型接入真实 agent client tool 调用，并通过 WebUI 验证一条真实 pi 端到端流程。

核心原则：

- Planner / RePlanner 提交 DAG 或 DAG Patch 必须走 Internal Agent Tool API，不再依赖最终自然语言/JSON 输出解析作为主路径。
- Worker 提交结果、阻塞、失败和 signal 必须走 Internal Agent Tool API。
- WebUI / External API 负责用户控制、观察、审批、异议和人工 DAG 修改。
- Backend service 是唯一 authoritative executor：agent 和 WebUI 都只能提交命令/提案/事实，真正 apply、调度、状态迁移由后端完成。
- Agent tools 不接受 `task_id` / `work_item_id` / `run_id` 作为权威身份参数；后端必须从当前 `session_id` / `turn_id` / runtime context 推导授权上下文。

## 目标

实现一条真实 pi 可验证链路：

```text
WebUI creates DAG task
  -> Backend starts pi Planner turn
  -> Planner calls submitPlan(initial_dag)
  -> Backend validates/applies DAG
  -> Scheduler dispatches WorkItemRun to pi Worker
  -> Worker calls submitResult / raiseSignal
  -> Backend updates DAG run/projection/task state
  -> WebUI observes DAG progress and allows human intervention
```

## 核心架构约定

- Agent-facing tools 走 `/internal/v1/agent-tools/*`。
- WebUI-facing control 走 `/external/v1/*`。
- Shared domain services 复用 DAG proposal、patch validation、run result、signal、scheduler 逻辑。
- Agent `submitPlan` 只提交 proposal；是否 auto-apply 由 backend policy 决定。
- Agent `submitResult` 只作用于当前 authorized WorkItemRun。
- Agent `raiseSignal` 只记录 signal 并触发 policy；不直接修改 DAG。
- WebUI 可以创建 human signal、pause/resume/interrupt/cancel/retry、创建/审批/应用 human DAG patch proposal。
- WebUI 手动修改 DAG 也必须走 proposal/patch/validate/apply 流程，不能直接改表。
- JSON-output fallback 可以保留为兼容/诊断路径，但真实验证主路径必须使用 tools API。

## 第一版范围

包含：

- Internal Agent Tool API skeleton
- Agent tool context resolver / authorization
- `getContext`
- `submitPlan`
- `submitResult`
- `raiseSignal`
- pi extension 注册并转发 llmparty tools
- DAG task External API / WebUI 最小入口
- WebUI 最小 DAG 观察能力复用和小幅补强
- 用户异议 / human signal 最小入口
- pause / resume / interrupt 最小控制入口
- 真实 pi 端到端验证

暂不包含：

- 完整可视化 graph editor
- 复杂 human approval UI
- 多用户权限/RBAC
- 复杂 policy engine
- 分布式 lease / worker pool
- 完整 artifact/work product 管理
- Claude Code tools 完整接入（可后续按同一 Internal API 接入）

---

# [x] WorkItem 1：Internal Agent Tool API 与上下文授权

## 目标

建立 agent tools 的 Internal HTTP API 和统一上下文解析逻辑。该节点不要求 pi extension 已接入，但必须能通过测试直接调用 API。

## 主要文件

新增：

- `src/application/agent_tools.rs`
- `src/transport/http/internal_agent_tools.rs`
- `tests/agent_tools_internal_api.rs`

修改：

- `src/application/mod.rs`
- `src/transport/http/mod.rs`
- `src/transport/http/internal.rs`（如需要复用 internal error/request helper）
- `src/application/views.rs`（如需要 tool response view）

## API

第一版可以采用统一路由：

```text
POST /internal/v1/agent-tools/{tool_name}
```

`tool_name` 支持：

```text
getContext
submitPlan
submitResult
raiseSignal
```

请求最小格式：

```json
{
  "session_id": "sess_xxx",
  "turn_id": "turn_xxx",
  "runtime_instance_id": "rt_xxx",
  "input": {}
}
```

响应最小格式：

```json
{
  "ok": true,
  "tool": "submitPlan",
  "result": {}
}
```

错误格式遵循现有 API error 风格。

## Context Resolver

新增：

```text
AgentToolContextResolver
AgentToolContext
AgentToolMode
```

建议模型：

```text
AgentToolContext:
  session_id
  turn_id
  client_type
  runtime_instance_id
  task_id
  mode

AgentToolMode:
  Planning { role: planner | replanner }
  Execution { run_id, work_item_id }
```

解析规则：

- 校验 session 存在且非 terminal。
- 校验 turn 属于 session。
- 校验 runtime binding 的 `runtime_instance_id` 与请求匹配。
- 校验 turn metadata 包含 `dag_managed = true`。
- planning turn 通过 metadata `dag_planning_role` 推导 planner/replanner。
- execution turn 通过 `work_item_runs.turn_id` 推导 current run/work item。
- 不信任 request.input 内任何 task/run/work item 身份字段。

## 完成标准

- API 能识别未知 tool 并返回 404/400。
- 缺少 session/turn/runtime_instance_id 时返回 invalid request。
- runtime_instance_id 不匹配时拒绝。
- 非 DAG-managed turn 调用 DAG tools 被拒绝。
- planning turn 和 execution turn 能解析出不同 mode。
- 测试覆盖 unauthorized cross-run/cross-task 尝试。

---

# [x] WorkItem 2：`getContext` Agent Tool

## 目标

实现 agent 获取当前 DAG-managed 上下文的工具。Planner/RePlanner 和 Worker 获得不同视图。

## 主要文件

修改：

- `src/application/agent_tools.rs`
- `src/application/queries.rs`
- `src/application/views.rs`
- `tests/agent_tools_internal_api.rs`

## 行为

Planning context 返回：

```text
mode = planning
role = planner | replanner
task
current DAG summary / current DAG when exists
open signals
relevant proposals
execution profiles
```

Execution context 返回：

```text
mode = execution
task
work_item
work_item_run
dependencies / upstream completed items
acceptance_criteria
open related signals
```

## 约束

- agent 不传 task id。
- response 可以先使用现有 view model，保持最小可用。
- 不暴露 Internal-only runtime diagnostics，除非明确需要。

## 完成标准

- Planner turn 调 `getContext` 返回 planning context。
- Worker turn 调 `getContext` 返回 execution context。
- Worker 不能看到无关 task 的 work items。
- 返回内容足够让 pi planner/worker 根据 prompt 调用后续 tool。

---

# [x] WorkItem 3：`submitPlan` Agent Tool

## 目标

让 Planner/RePlanner 通过 Internal Agent Tool API 提交 initial DAG 或 DAG patch，后端完成保存、验证、auto-apply 和调度。

## 主要文件

修改：

- `src/application/agent_tools.rs`
- `src/application/dag_planning.rs`
- `src/application/dag.rs`
- `src/application/dag_scheduler.rs`
- `tests/agent_tools_internal_api.rs`
- `tests/dag_planner_flow.rs`（必要时补充）

## 输入

`initial_dag`：

```json
{
  "mode": "initial_dag",
  "summary": "Plan summary",
  "dag": {
    "work_items": [],
    "edges": []
  },
  "assumptions": [],
  "risks": []
}
```

`patch`：

```json
{
  "mode": "patch",
  "summary": "Patch summary",
  "patch": {
    "operations": []
  }
}
```

## 行为

Planner：

```text
submitPlan(initial_dag)
  -> validate planning context role = planner
  -> save dag_proposal
  -> validate initial DAG
  -> policy auto-apply first version
  -> apply_initial_dag
  -> mark proposal applied
  -> task state running
  -> scheduler.schedule_task(task_id)
```

RePlanner：

```text
submitPlan(patch)
  -> validate planning context role = replanner
  -> save patch proposal
  -> validate patch
  -> policy auto-apply first version
  -> apply_patch
  -> acknowledge triggering signal when available
  -> task state running
  -> scheduler.schedule_task(task_id)
```

## 约束

- Worker execution turn 不能调用 `submitPlan`。
- Planner 不能提交 patch；RePlanner 不能提交 initial DAG，除非后续 policy 明确允许。
- 无效 DAG 必须 rejected，不应产生 partial apply。
- 第一版 auto-apply，但必须保留 proposal state 以便后续人工审批。

## 完成标准

- pi-equivalent API call 能创建并应用 initial DAG。
- API response 返回 `proposal_id`、validation/apply 结果、scheduler outcome。
- invalid DAG rejected。
- repeated submitPlan 需要具备基本幂等/防重复策略：同一 turn 已提交成功后再次提交返回已提交结果或 state conflict。
- WebUI 刷新后能看到 WorkItems/Runs。

---

# [x] WorkItem 4：`submitResult` Agent Tool

## 目标

让 Worker 通过 tool API 提交当前 WorkItemRun 的结果，替代从 final output JSON 解析作为主路径。

## 主要文件

修改：

- `src/application/agent_tools.rs`
- `src/application/dag_run_result.rs`
- `src/application/events.rs`（保持 event fallback 兼容）
- `tests/agent_tools_internal_api.rs`
- `tests/dag_run_result.rs`

## 输入

```json
{
  "status": "completed",
  "summary": "Work completed",
  "outputs": [
    {
      "kind": "document",
      "name": "demo file",
      "summary": "Created demo file",
      "ref": "file:/tmp/llmparty-pi-dag-demo.txt"
    }
  ],
  "failure": null
}
```

支持 status：

```text
completed
failed
blocked
needs_input
```

## 行为

```text
submitResult
  -> resolve execution context by session/turn
  -> validate run is current and non-terminal
  -> update work_item_runs
  -> update work_item_runtime_projection
  -> record outputs metadata first version in run summary/failure or event payload
  -> record dag.run_completed task event
  -> if completed: scheduler.schedule_task(task_id)
  -> if blocked/needs_input/failed: aggregate task state or trigger policy
```

## 约束

- Planning turn 不能调用 `submitResult`。
- Worker 不能提交别的 run result。
- terminal run 重复提交必须 idempotent 或 state conflict，不能二次推进 scheduler。
- Internal event `turn.completed` fallback 保留，但真实 tool path 应可独立完成 run 状态更新。

## 完成标准

- Worker tool call 后 WorkItemRun completed。
- 上游 completed 能解锁下游并调度。
- failed/blocked/needs_input 能正确反映 task/work item state。
- 重复提交不重复创建下游 active run。

---

# [x] WorkItem 5：`raiseSignal` Agent Tool

## 目标

让 Planner/Worker/RePlanner 可以通过 tool API 记录 signal，并由 backend policy 决定是否触发 RePlanner 或等待用户。

## 主要文件

修改：

- `src/application/agent_tools.rs`
- `src/application/dag_run_result.rs` 或新增 `src/application/dag_signals.rs`
- `src/application/dag_planning.rs`
- `src/application/queries.rs`
- `tests/agent_tools_internal_api.rs`
- `tests/dag_run_result.rs`

## 输入

```json
{
  "kind": "replan_requested",
  "summary": "Need one more work item",
  "detail": "Current DAG is missing validation",
  "severity": "medium",
  "related_refs": []
}
```

支持 kind：

```text
needs_input
replan_requested
risk
missing_dependency
scope_change
assistance_needed
review_requested
other
```

## 行为

```text
raiseSignal
  -> resolve context
  -> insert dag_signals with source_session_id/current run if any
  -> record task event
  -> if kind = replan_requested: start replanning turn by policy
  -> else update task state only if policy requires
```

## 约束

- signal 不直接修改 DAG。
- Human/user signal 走 External API，不走 Internal Agent Tool API。
- signal source 必须区分 agent/human/system。

## 完成标准

- Worker 调 `raiseSignal(replan_requested)` 后 WebUI signals 区域可见。
- replan_requested 能启动 replanner pi turn。
- needs_input 能让 task 进入可人工处理状态或显示 open signal。

---

# [x] WorkItem 6：pi Extension 注册 llmparty Agent Tools

## 目标

在 pi extension 中注册 agent-visible llmparty tools，并把调用安全转发到 Internal Agent Tool API。

## 主要文件

修改：

- `clients/pi/src/tools.ts`
- `clients/pi/src/index.ts`
- `clients/pi/src/context.ts`
- `clients/pi/test/tools.test.ts`
- `clients/pi/README.md`

可能新增：

- `clients/pi/src/agentTools.ts`
- `clients/pi/test/agentTools.test.ts`

## 工具

注册：

```text
llmparty.getContext
llmparty.submitPlan
llmparty.submitResult
llmparty.raiseSignal
```

如果 pi tool API 不支持 namespace，可使用：

```text
llmparty_getContext
llmparty_submitPlan
llmparty_submitResult
llmparty_raiseSignal
```

但 schema/source of truth 仍以 `clients/tools/llmparty-tools.v1.json` 为准。

## 转发逻辑

每个 tool handler：

```text
load current turn context from LLMPARTY_CURRENT_TURN_FILE / env
build request { session_id, turn_id, runtime_instance_id, input }
POST internalEventUrl base converted to /internal/v1/agent-tools/{tool}
return backend result to pi agent
append diagnostics on failure
```

## 约束

- pi extension 不理解 DAG 业务逻辑，只做 context loading + HTTP forwarding。
- 工具不可在缺少 llmparty context 时注册，或注册后调用时返回明确 unavailable。
- 不能把 token/secrets 打到 agent-visible result。
- 保留现有 event hooks：session.ready、turn.output、turn.completed 仍用于生命周期观察。

## 完成标准

- 单元测试证明 tools 被注册。
- tool handler 会 POST 正确 Internal API。
- 后端 error 能作为 tool error 返回。
- README 描述真实 DAG tool flow。

---

# [x] WorkItem 7：DAG Task External API 与 WebUI 最小入口

## 目标

提供一个明确入口从 WebUI 创建真实 DAG task，并启动 pi Planner turn。避免复用旧 workspace routing task flow 造成语义混淆。

## 主要文件

新增：

- `src/transport/http/external/dag_tasks.rs`（或并入现有 external dag handler，但保持清晰）
- `tests/dag_task_api.rs`

修改：

- `src/transport/http/mod.rs`
- `src/transport/http/external/mod.rs`
- `src/application/tasks/commands.rs`（仅复用创建 task 基础逻辑，避免大改旧 routing）
- `apps/web/src/api/types.ts`
- `apps/web/src/api/client.ts`
- `apps/web/src/stores/tasks.ts`
- `apps/web/src/components/tasks/CreateTaskForm.svelte`

## API

新增：

```text
POST /external/v1/dag-tasks
```

请求：

```json
{
  "input": "Create demo file",
  "workspace": "/tmp/llmparty-demo",
  "client_type": "pi",
  "metadata": {}
}
```

行为：

```text
create task with DAG mode metadata
upsert/link workspace
set task state = planning
DagPlanningService::start_initial_planning(task_id)
return task + planning session/turn summary
```

## WebUI

`CreateTaskForm.svelte` 增加模式选择：

```text
Task mode:
  Normal task
  DAG task
```

DAG task 模式：

- workspace 必填。
- client_type 默认 `pi`。
- 调用 `/external/v1/dag-tasks`。
- 创建后选中 task，并尽量打开 planner session。

## 完成标准

- WebUI 能创建 DAG task。
- 后端创建 pi planner session/turn。
- Task detail 显示 planning 状态和 task events。
- Planner pi 收到 prompt，并可调用 `llmparty.submitPlan`。

---

# [x] WorkItem 8：WebUI Human Control 最小边界

## 目标

实现用户在 WebUI 中的最小干预能力：打断、暂停/恢复、提出异议。手动 DAG patch editor 留到后续，但 API 边界先预留。

## 主要文件

新增/修改：

- `src/transport/http/external/dag.rs`
- `src/application/tasks/commands.rs` 或新增 `src/application/human_controls.rs`
- `src/application/views.rs`
- `tests/dag_human_control_api.rs`
- `apps/web/src/api/types.ts`
- `apps/web/src/api/client.ts`
- `apps/web/src/stores/tasks.ts`
- `apps/web/src/components/tasks/TaskDetail.svelte`

## API

新增：

```text
POST /external/v1/tasks/{task_id}/pause
POST /external/v1/tasks/{task_id}/resume
POST /external/v1/tasks/{task_id}/signals
```

已有 interrupt/cancel 可复用，如语义不足再补：

```text
POST /external/v1/tasks/{task_id}/interrupt
POST /external/v1/tasks/{task_id}/cancel
```

Human signal 请求：

```json
{
  "kind": "user_objection",
  "summary": "Plan is too broad",
  "detail": "Please reduce to one implementation step",
  "severity": "medium"
}
```

## 行为

- pause：阻止 scheduler 派发新的 WorkItemRun，不一定中断当前 turn。
- resume：解除 pause 并可触发 scheduler tick。
- human signal：写入 `dag_signals`，source = human，WebUI 可见；第一版可选择触发 replanner 或仅等待用户手动 resume/replan。
- interrupt：中断当前 running turn/session。

## 完成标准

- WebUI 可以提交用户异议，Signals 区域可见。
- pause 后 scheduler tick 不派发新 run。
- resume 后 scheduler 可继续派发。
- interrupt 当前 turn 后 run/task 状态可观察。

---

# [ ] WorkItem 9：Human DAG Patch Proposal API（最小手动修改 DAG）

## 目标

建立 WebUI 手动修改 DAG 的后端边界：用户提交 patch proposal，后端 validate，用户 apply。第一版可以不做复杂图形编辑器。

## 主要文件

新增/修改：

- `src/application/dag_human_proposals.rs` 或扩展 `src/application/dag.rs`
- `src/transport/http/external/dag.rs`
- `tests/dag_human_patch_api.rs`
- `apps/web/src/api/types.ts`
- `apps/web/src/api/client.ts`
- `apps/web/src/components/tasks/TaskDetail.svelte`（可先用 textarea JSON patch）

## API

```text
POST /external/v1/tasks/{task_id}/dag-proposals
POST /external/v1/tasks/{task_id}/dag-proposals/{proposal_id}/apply
POST /external/v1/tasks/{task_id}/dag-proposals/{proposal_id}/reject
```

请求：

```json
{
  "summary": "Add validation step",
  "patch": {
    "operations": []
  }
}
```

## 行为

```text
human submits patch proposal
  -> validate patch shape and impact
  -> store proposal source = human
  -> pending_approval or validated
user applies
  -> apply_patch
  -> scheduler recompute
```

## 约束

- 不允许直接改 authoritative DAG。
- apply 时必须检查 running/completed work item impact。
- 第一版 patch operations 可限制在 add_work_item/add_edge/update_work_item 这类低风险操作。

## 完成标准

- 用户可通过 WebUI textarea 提交 patch proposal。
- invalid patch rejected。
- valid patch apply 后 WebUI DAG 更新。
- scheduler recompute 后新 ready item 可运行。

---

# [ ] WorkItem 10：真实 pi 端到端验证

## 目标

用真实 pi client、真实 tools、真实 WebUI 验证完整后端流程。

## 准备

启动：

```bash
LLMPARTY_EXTERNAL_API_TOKEN=dev-token \
LLMPARTY_INTERNAL_EVENT_URL=http://127.0.0.1:8080/internal/v1/events \
LLMPARTY_PI_TUI_COMMAND='pi -e /absolute/path/to/llmparty/clients/pi' \
LLMPARTY_DATABASE_URL=sqlite:///tmp/llmparty-pi-dag-demo.db \
cargo run
```

打开：

```text
http://127.0.0.1:8080/dashboard
```

## 验证任务

在 WebUI 创建 DAG task：

```text
请创建 /tmp/llmparty-pi-dag-demo.txt，内容为 hello from llmparty dag。请先规划成一个最小 DAG，然后执行。
```

Planner 应通过 tool 提交 initial DAG，至少一个 WorkItem：

```text
Create demo file
```

Worker 应通过 tool 提交 result：

```json
{
  "status": "completed",
  "summary": "Created /tmp/llmparty-pi-dag-demo.txt",
  "outputs": [
    {
      "kind": "document",
      "name": "demo file",
      "ref": "file:/tmp/llmparty-pi-dag-demo.txt"
    }
  ]
}
```

## WebUI 验收

必须看到：

- Task 创建后进入 planning/running。
- Planner session/turn 存在。
- DAG summary 非空。
- WorkItem 有 run/session/turn。
- WorkItem 从 running 到 completed。
- Task 最终 completed。
- Task events 包含 plan submitted / dag applied / run completed。
- Signals 区域可显示 agent/human signal。

## 文件验收

```bash
cat /tmp/llmparty-pi-dag-demo.txt
```

期望：

```text
hello from llmparty dag
```

## 最终验证命令

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
pnpm --dir apps/web typecheck
pnpm --dir apps/web build
pnpm --dir clients/pi test
pnpm --dir clients/pi typecheck
pnpm --dir clients/claude-code test
pnpm --dir clients/claude-code typecheck
```

## 完成标准

- 真实 pi planner 通过 `submitPlan` tool 交付 DAG。
- 真实 pi worker 通过 `submitResult` tool 交付结果。
- 后端不依赖 final output JSON 作为主路径完成 DAG apply/result sync。
- WebUI 能观察完整流程。
- 用户可以提交 human signal，并能 pause/resume/interrupt。
- 所有最终验证命令通过。

---

# 实施策略

## 推荐顺序

1. Internal Agent Tool API skeleton + context resolver
2. `submitPlan`
3. pi extension 注册 `submitPlan`
4. DAG task External API / WebUI 创建入口
5. `submitResult`
6. pi extension 注册 `submitResult`
7. `getContext`
8. `raiseSignal`
9. WebUI human signal / pause / resume
10. Human patch proposal
11. 真实 pi E2E 验证

## 中间验证策略

每个 WorkItem 至少运行对应局部测试：

```bash
cargo test <test_name>
pnpm --dir clients/pi test
pnpm --dir apps/web typecheck
```

涉及 Rust API 改动后至少运行：

```bash
cargo fmt
cargo test agent_tools_internal_api
```

## 关键风险

- pi extension tool registration API 细节可能与预期不同，需要先用最小测试确认。
- runtime_instance_id 校验必须准确，否则 tool API 容易被错误 session 调用。
- `submitPlan` 和 `turn.completed` fallback 可能双写，必须设计幂等或明确 tool path 优先。
- WebUI pause/resume 需要 scheduler respect paused state，否则用户控制无效。
- Human patch apply 影响 running/completed WorkItem，第一版应限制高风险操作。

## 后续增强

完成本 milestone 后再考虑：

- Claude Code plugin tools 接入
- 完整 proposal approval UI
- DAG graph visualization/editor
- policy engine：auto-apply / require approval / risk threshold
- retry policy / lease / timeout
- artifact/work product first-class model
- patch impact analysis
- RBAC / multi-user audit
