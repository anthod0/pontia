# 全局任务 / Workspace 数据设计

日期：2026-05-02

## 目标

设计新的 llmparty 数据方案，使其能够支撑以下目标：

1. 一个 WebUI 管理所有 workspace。
2. 用户只需要发布全局 task，不需要手动选择 session。
3. llmparty 自动把 task 关联到合适的 workspace、session 和 turn。
4. llmparty 产生的数据库和运行状态文件不污染真实项目目录。
5. 新方案不需要兼容当前把运行状态文件写入项目内 `.llmparty/` 的旧设计。

## 核心概念

### Task

Task 是用户提交的全局任务，是 WebUI 中最主要的一等对象。

Task 可以先于 workspace / session 存在。也就是说，用户发布 task 后，即使系统还没有判断应该在哪个 workspace 中执行，这个 task 也应该已经被持久化并可以在 WebUI 中看到。

### Workspace

Workspace 是执行上下文，通常对应一个项目目录。

Workspace 不是数据分区边界，也不应该保存 llmparty 自己的状态文件。它主要用于：

- 作为 runtime 的工作目录，也就是 cwd
- 作为 task routing 的目标
- 作为 artifact discovery 的文件来源

### Session

Session 是一个长期运行的 agent runtime 实例。

一个 session 绑定一个 workspace，但一个 workspace 可以有多个 session。例如：

- 不同 client type 的 session
- 并行执行不同任务的 session
- 旧 session 保留历史，新 session 重新开始

### Turn

Turn 是某个 session 内的一次具体执行。

Task 是用户意图，turn 是这个意图被投递到 session 后产生的实际执行记录。

## 对象关系

```text
workspace 1 -> N sessions
session   1 -> N turns
task      1 -> 0/1 workspace
task      1 -> 0/1 session
task      1 -> 0/1 turn
```

典型执行流程：

```text
用户创建 task
  -> task 先全局持久化
  -> router 推断 workspace
  -> scheduler 选择或创建 session
  -> task 被 dispatch 成 turn
  -> task 记录 workspace_id、session_id、turn_id
```

## 文件布局

所有 llmparty 自己拥有的持久化数据和 runtime 状态文件都应该放在用户数据目录下：

```text
~/.local/share/llmparty/
  llmparty.db
  runtimes/
    <session_id>/
      runtime.sh
      runtime.log
      current-turn.json
      pi-hook.log
      adapter-events.jsonl
  tmp/
```

真实项目目录只作为 runtime 的工作目录：

```text
/path/to/project
```

llmparty 不应再创建：

```text
/path/to/project/.llmparty/
```

runtime 启动时，环境变量应该指向全局 runtime 状态目录：

```bash
LLMPARTY_WORKSPACE=/path/to/project
LLMPARTY_RUNTIME_DIR=~/.local/share/llmparty/runtimes/<session_id>
LLMPARTY_CURRENT_TURN_FILE=~/.local/share/llmparty/runtimes/<session_id>/current-turn.json
LLMPARTY_RUNTIME_LOG=~/.local/share/llmparty/runtimes/<session_id>/runtime.log
LLMPARTY_ADAPTER_EVENT_LOG=~/.local/share/llmparty/runtimes/<session_id>/adapter-events.jsonl
LLMPARTY_PI_HOOK_LOG=~/.local/share/llmparty/runtimes/<session_id>/pi-hook.log
```

tmux / runtime 的 cwd 仍然是 workspace 路径。

## 数据库位置

默认数据库建议为：

```text
sqlite://~/.local/share/llmparty/llmparty.db
```

实现时可以在打开 SQLite 前把 `~` 展开成绝对路径。

## 数据表设计

### workspaces

`workspaces` 用于保存所有已知执行上下文，并支撑 WebUI 对 workspace 的全局管理。

```sql
CREATE TABLE workspaces (
    workspace_id TEXT PRIMARY KEY NOT NULL,
    canonical_path TEXT NOT NULL UNIQUE,
    display_path TEXT NOT NULL,
    name TEXT,
    state TEXT NOT NULL DEFAULT 'active',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_used_at TEXT
);

CREATE INDEX idx_workspaces_last_used
ON workspaces(last_used_at, workspace_id);
```

字段说明：

- `workspace_id`：workspace 的稳定 ID。
- `canonical_path`：规范化后的绝对路径，用于查找和去重。
- `display_path`：用于 UI 展示的路径，可以保留用户输入形式或更友好的形式。
- `name`：可选名称，例如 repo 名。
- `state`：workspace 状态，例如 `active` / `archived`。
- `metadata`：扩展信息，例如 repo 信息、默认 client type、语言栈等。
- `last_used_at`：最近被 task/session 使用的时间。

### tasks

`tasks` 是全局任务池的核心表。

```sql
CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL,
    input TEXT NOT NULL,

    workspace_id TEXT,
    session_id TEXT,
    turn_id TEXT,

    routing_state TEXT NOT NULL DEFAULT 'pending',
    routing_reason TEXT,
    routing_confidence REAL,

    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id),
    FOREIGN KEY(turn_id) REFERENCES turns(turn_id)
);

CREATE INDEX idx_tasks_state_created
ON tasks(state, created_at, task_id);

CREATE INDEX idx_tasks_workspace
ON tasks(workspace_id, created_at, task_id);

CREATE INDEX idx_tasks_session
ON tasks(session_id, created_at, task_id);
```

推荐 task 状态：

```text
created
routing
needs_confirmation
queued
running
completed
failed
cancelled
```

推荐 routing 状态：

```text
pending
matched
ambiguous
failed
confirmed
```

这样即使 workspace 推断失败，task 也有明确落点。例如：

```text
tasks.workspace_id IS NULL
tasks.session_id IS NULL
tasks.turn_id IS NULL
state = 'needs_confirmation'
routing_state = 'ambiguous'
```

WebUI 可以展示这个 task，并提示用户确认 workspace。

### task_events

`task_events` 记录 task 的路由过程和生命周期变化。

```sql
CREATE TABLE task_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    FOREIGN KEY(task_id) REFERENCES tasks(task_id)
);

CREATE INDEX idx_task_events_task
ON task_events(task_id, created_at, event_id);
```

示例事件：

```text
task.created
task.routing_started
task.workspace_matched
task.routing_ambiguous
task.session_selected
task.session_created
task.turn_created
task.running
task.completed
task.failed
```

相比把所有路由过程都塞进 `tasks.metadata`，单独的 `task_events` 更利于审计、调试和 WebUI 展示。

### sessions

Session 是绑定到某个 workspace 的 agent runtime 实例。

```sql
CREATE TABLE sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    client_type TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    state TEXT NOT NULL,
    current_turn_id TEXT,
    state_version INTEGER NOT NULL DEFAULT 0,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
);

CREATE INDEX idx_sessions_workspace
ON sessions(workspace_id, state, updated_at, session_id);
```

设计原则：

- `workspace_id` 是 session 到 workspace 的权威关联。
- session 不再保存项目内 `.llmparty` 路径。
- runtime 文件路径要么保存在 `runtime_bindings.metadata`，要么通过 `session_id` 和全局 data dir 确定性推导。

### runtime_bindings

`runtime_bindings` 负责把 session 绑定到具体 runtime 进程。

```sql
CREATE TABLE runtime_bindings (
    session_id TEXT PRIMARY KEY NOT NULL,
    runtime_kind TEXT NOT NULL,
    runtime_ref TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    FOREIGN KEY(session_id) REFERENCES sessions(session_id)
);
```

`metadata` 需要稳定包含以下字段：

```json
{
  "backend": "tmux",
  "tmux_session": "llmparty_<session_id>",
  "workspace": "/path/to/project",
  "runtime_dir": "/home/user/.local/share/llmparty/runtimes/<session_id>",
  "runtime_log": "/home/user/.local/share/llmparty/runtimes/<session_id>/runtime.log",
  "adapter_event_log": "/home/user/.local/share/llmparty/runtimes/<session_id>/adapter-events.jsonl",
  "current_turn_file": "/home/user/.local/share/llmparty/runtimes/<session_id>/current-turn.json",
  "pi_hook_log": "/home/user/.local/share/llmparty/runtimes/<session_id>/pi-hook.log"
}
```

### turns、events、artifacts

现有的 turn / event / artifact 概念仍然有价值，但在新模型里它们是执行层记录，而不是用户面对的全局任务入口。

原则：

- `turns` 属于 session。
- `events` 属于 session，可选关联 turn。
- `artifacts` 属于 session，可选关联 turn。
- task 到 turn 的关联由 `tasks.turn_id` 表达。
- artifact 可以引用真实 workspace 中的文件，但 llmparty 不应把自己的 runtime 状态文件写进 workspace。

## 双向查找路径

### 通过 session_id 查 workspace

```sql
SELECT w.*
FROM sessions s
JOIN workspaces w ON w.workspace_id = s.workspace_id
WHERE s.session_id = ?;
```

### 通过 workspace 查 sessions

```sql
SELECT s.*
FROM sessions s
JOIN workspaces w ON w.workspace_id = s.workspace_id
WHERE w.canonical_path = ?
ORDER BY s.updated_at DESC;
```

### 通过 task 查完整执行上下文

```sql
SELECT t.*, w.*, s.*, tr.*
FROM tasks t
LEFT JOIN workspaces w ON w.workspace_id = t.workspace_id
LEFT JOIN sessions s ON s.session_id = t.session_id
LEFT JOIN turns tr ON tr.turn_id = t.turn_id
WHERE t.task_id = ?;
```

### 通过 workspace 查 tasks

```sql
SELECT t.*
FROM tasks t
JOIN workspaces w ON w.workspace_id = t.workspace_id
WHERE w.canonical_path = ?
ORDER BY t.created_at DESC;
```

### 通过 session_id 查 runtime 状态文件

```sql
SELECT metadata
FROM runtime_bindings
WHERE session_id = ?;
```

从 `metadata.runtime_dir` 或相关字段得到：

```text
runtime.sh
runtime.log
current-turn.json
pi-hook.log
adapter-events.jsonl
```

## WebUI 数据模型

WebUI 应该采用 task-first 设计。

主要页面：

```text
Tasks       全局任务池，主要入口
Workspaces  所有已知执行上下文，用于管理和调试
Sessions    runtime 实例列表，用于调试和生命周期控制
```

普通用户流程：

```text
用户输入 task
  -> task 出现在全局 inbox
  -> router 关联 workspace
  -> scheduler 关联 session
  -> turn 开始执行
  -> task 状态持续更新
```

Workspace 和 Session 页面是辅助视图，不应该成为用户提交任务时必须理解的主路径。

## 路由边界

本文档定义的是数据关系和持久化方案，不定义最终 workspace 推断算法。

数据库必须支持尚未完成路由的 task：

```text
tasks.workspace_id IS NULL
tasks.session_id IS NULL
tasks.turn_id IS NULL
state = 'routing' 或 'needs_confirmation'
```

这样后续可以无痛支持不同路由策略：

- 根据用户输入中的项目名或路径匹配
- 根据最近使用 workspace 匹配
- 根据文件 / repo 索引匹配
- 根据 embedding 相似度匹配
- 路由不确定时请求用户确认

## Runtime 目录推导

runtime 目录应该是确定性的：

```text
runtime_dir = <llmparty_data_dir>/runtimes/<session_id>
```

数据库可以在 `runtime_bindings.metadata.runtime_dir` 中保存该路径，但该路径也可以通过 `session_id` 和全局 data dir 重新计算。

权威关系必须在数据库中，而不是依赖文件系统目录结构。

文件系统只负责保存 runtime 的进程局部状态文件。

## 推荐实施顺序

虽然新方案不要求兼容旧设计，但实施时仍建议分阶段落地，降低风险。

### 阶段 1：迁移 llmparty 自有文件位置

- 默认数据库迁移到 `~/.local/share/llmparty/llmparty.db`。
- runtime 状态文件迁移到 `~/.local/share/llmparty/runtimes/<session_id>/`。
- workspace 只作为 runtime cwd。
- 不再创建项目内 `.llmparty/`。

### 阶段 2：引入 workspaces 表

- 创建 session 或路由 task 时 canonicalize workspace。
- upsert workspace。
- session 通过 `workspace_id` 关联 workspace。
- WebUI 可以展示和管理全局 workspace 列表。

### 阶段 3：引入 tasks 表

- 用户提交 task 后先写入 `tasks`。
- 路由完成后更新 `workspace_id`。
- session 选择或创建完成后更新 `session_id`。
- turn 创建后更新 `turn_id`。
- WebUI 从 session-first 逐步切换到 task-first。

## 非目标

本文档不要求兼容项目内 `.llmparty/` 状态目录。

本文档不定义最终 workspace 自动推断算法。

本文档不要求一个 workspace 只能有一个 session。多个 session 共享同一个 workspace 是允许且预期的。

本文档不要求把 artifact 文件复制到 llmparty 数据目录。Artifact 可以继续引用 workspace 中的真实文件；需要避免的是 llmparty 自己的数据库和 runtime 状态文件污染 workspace。
