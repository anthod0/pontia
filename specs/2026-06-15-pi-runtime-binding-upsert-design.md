# pi TUI 与 pontia session 强一致绑定设计

日期：2026-06-15

## 背景

pontia 的目标是让 Web UI、TUI、移动端或其他控制面都只是同一个 agent runtime 的入口。用户无论从 pi TUI 输入，还是从 pontia Web UI 输入，都应该作用于同一个 pontia session，并通过同一套 session/turn/event 模型观察和控制。

当前 pontia 已支持由 pontia 启动 tmux-backed pi runtime，并通过 pi extension 上报 `session.ready`、`turn.started`、`turn.output`、`turn.completed` 等事实。但用户也可能直接打开 pi TUI，且该 pi 可能：

- 在 pontia 创建的 tmux session 中；
- 在用户自己的 tmux session 的任意 window/pane 中；
- 在非 tmux 普通终端中。

本设计希望统一这些入口，避免为 Web UI 入口和 TUI 入口维护两套业务逻辑。

## 设计目标

1. **pi extension 启动即绑定 pontia session**
   - 只要 pi TUI 加载 pontia pi extension，就在 `session_start` 时 upsert pontia session。
   - 同一个 pi client session 重启 extension 时，应复用同一个 pontia session，而不是重复创建。

2. **入口行为一致**
   - Web UI 输入和 TUI 输入都应进入同一套 pontia session/turn/event 流。
   - 不因为用户入口不同而分裂业务逻辑。
   - 差异只由 runtime capabilities 表达。

3. **非 tmux session 仅 Web 不可写**
   - 非 tmux pi TUI 由于 pontia 无法控制终端输入，Web UI 输入框应禁用。
   - 这不是安全限制，也不代表非 tmux session 是二等 session。
   - 除 Web 不可写外，非 tmux session 应和 tmux session 一样展示 turns、输出、usage、transcript、artifacts 等。

4. **通过 pi extension 事件上报事实**
   - 尽可能通过 pi lifecycle events 上报 pontia domain events。
   - 避免在 Web UI dispatch 前后手写大量 special-case API 调用。
   - Web UI dispatch 最终也应由 pi 的 `agent_start` / `agent_end` 等事件确认事实状态。

5. **restart 优先使用用户实际 start command**
   - Web UI restart 时优先使用用户手动输入过或 extension 上报的 start command。
   - 只有没有 start command 时才 fallback 到 pontia config 默认命令。

## 非目标

- 第一版不要求非 tmux session 支持 Web 写入。
- 第一版不要求将 `runtime_bindings.metadata` 中所有字段结构化。
- 第一版不通过解析 TUI 屏幕内容或 runtime log 推断 turn 完成状态。

## 总体模型

pontia session 是唯一的控制状态。pi TUI 和 Web UI 都只是同一个 runtime 的控制入口。

```text
TUI 输入 ─┐
          ├─> pi runtime ─> pi extension events ─> pontia events ─> projections/Web UI
Web 输入 ─┘
```

对于 Web UI 输入：

```text
Web UI submit
  ↓
pontia 校验 session capability
  ↓
如果可写：写 current-turn context + dispatch 到 runtime
  ↓
pi TUI 实际开始 agent turn
  ↓
pi extension agent_start / agent_end 上报事实
```

对于 TUI 输入：

```text
用户直接在 pi TUI 输入
  ↓
pi extension agent_start 发现 turn
  ↓
如果没有有效 current-turn context，创建/分配新 turn
  ↓
上报 turn.created / turn.started / output / completed
```

## runtime_bindings schema 扩展

当前 `runtime_bindings` 表里有历史字段 `runtime_ref TEXT NOT NULL`。这个字段名过于泛化，容易把“控制谁”和“怎么控制”重新抽象成多分支模型，因此新设计应移除它的业务语义，并通过新 migration 将表结构迁移为语义明确的字段。

目标表结构：

```sql
CREATE TABLE runtime_bindings_new (
    session_id TEXT PRIMARY KEY NOT NULL,
    runtime_kind TEXT NOT NULL,
    runtime_instance_id TEXT,
    start_command TEXT,
    launch_cwd TEXT,
    last_seen_at TEXT,

    tmux_socket_path TEXT,
    tmux_pane_id TEXT,

    metadata TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id)
);
```

迁移方式：追加新 migration，重建 `runtime_bindings` 表：

1. 创建 `runtime_bindings_new`。
2. 从旧表复制数据；旧 `runtime_ref` 不再复制为顶层字段，如需诊断可写入 `metadata.legacy_runtime_ref`。
3. tmux session/window/pane index 等辅助信息放入 `metadata.tmux`，顶层只保留 `tmux_socket_path` 和 `tmux_pane_id`。
4. 删除旧表并 rename 新表为 `runtime_bindings`。
5. 后续代码禁止新增对 `runtime_ref` 的依赖。

### 字段语义

| 字段 | 说明 |
| --- | --- |
| `runtime_kind` | runtime 类型。pi TUI 统一使用 `pi_tui` |
| `runtime_instance_id` | 当前 runtime/extension 实例 id，用于事件授权和 stale event 过滤 |
| `start_command` | 用于 restart 的启动命令，优先记录用户实际命令 |
| `launch_cwd` | pi 启动 cwd |
| `last_seen_at` | extension heartbeat/session_start/事件上报刷新时间 |
| `tmux_socket_path` | tmux socket path；Web UI 通过 tmux 注入输入时必需 |
| `tmux_pane_id` | pi TUI 所在 pane id，例如 `%42`；这是 Web 输入的唯一明确控制目标 |
| `metadata` | 保留 runtime/client/诊断扩展信息；tmux session/window/pane index 等辅助信息也放这里 |

### metadata 保留内容

`metadata` 继续用于不适合固化进 schema 的信息，例如：

```json
{
  "client_session_key": "pi_xxx",
  "client_session_file": "/path/to/session.jsonl",
  "client_session_dir": "/path/to/session-dir",
  "client_cwd": "/project",
  "runtime_dir": "...",
  "current_turn_file": ".../current-turn.json",
  "adapter_event_log": ".../adapter-events.jsonl",
  "internal_event_url": "http://127.0.0.1:8080/internal/v1/events",
  "tmux": {
    "session_id": "$1",
    "session_name": "dev",
    "window_id": "@3",
    "window_index": 0,
    "pane_index": 1,
    "pane_current_path": "/project"
  },
  "diagnostics": {
    "pi_hook_log": ".../pi-hook.log"
  },
  "capabilities": {
    "accept_task": true,
    "report_turn_started": true,
    "report_turn_finished": true,
    "interrupt": true,
    "stream_output": true,
    "heartbeat": true,
    "artifact_sources": true,
    "context_usage": "exact"
  }
}
```

### 示例：pontia 启动或用户在 tmux 中手动打开 pi

只要 pi TUI 位于 tmux pane 中，记录 pane 位置。Web UI 输入统一通过该 pane 注入，不区分 managed/attached dispatch 分支。

```text
runtime_kind = "pi_tui"
tmux_socket_path = "/tmp/tmux-1000/default"
tmux_pane_id = "%42"

metadata.tmux.session_id = "$1"
metadata.tmux.session_name = "dev"
metadata.tmux.window_id = "@3"
metadata.tmux.window_index = 0
metadata.tmux.pane_index = 1
```

对应 capability：

```json
{
  "accept_task": true,
  "interrupt": true,
  "stream_output": true,
  "report_turn_started": true,
  "report_turn_finished": true
}
```

### 示例：非 tmux 中手动打开 pi

非 tmux session 没有 `tmux_socket_path` / `tmux_pane_id`，因此 Web UI 不可写，但其他事件上报和展示行为保持一致。

```text
runtime_kind = "pi_tui"
tmux_socket_path = NULL
tmux_pane_id = NULL
```

对应 capability：

```json
{
  "accept_task": false,
  "interrupt": false,
  "stream_output": true,
  "report_turn_started": true,
  "report_turn_finished": true
}
```

## pi extension 重构

### session_start：upsert pontia session

pi extension 在 `session_start` 时执行绑定：

1. 读取 pi session 信息：
   - `ctx.sessionManager.getSessionId()`
   - `ctx.sessionManager.getSessionFile()`
   - `ctx.sessionManager.getSessionDir()`
   - `ctx.sessionManager.getCwd()`
2. 读取 tmux 信息：
   - `process.env.TMUX`
   - `process.env.TMUX_PANE`
3. 如果在 tmux 中，通过命令补全信息：

   ```bash
   tmux -S "$socket_path" display-message -p -t "$TMUX_PANE" \
     '#{session_id} #{session_name} #{window_id} #{window_index} #{pane_id} #{pane_index} #{pane_current_path}'
   ```

4. 调用 pontia upsert API。
5. 获得 `session_id`、`runtime_instance_id`、`internal_event_url`、`current_turn_file`、capabilities。
6. 后续所有事件使用该 pontia session context 上报。

如果 pontia server 不可达：

- 不阻塞 pi 启动；
- 记录诊断；
- 周期性重试绑定；
- 绑定成功后再开始向 pontia 上报后续事实。

### upsert API

建议新增 internal API：

```text
POST /internal/v1/runtime-bindings/upsert
```

请求示例：

```json
{
  "client_type": "pi",
  "client_session_key": "pi_xxx",
  "client_session_file": "/path/to/session.jsonl",
  "client_session_dir": "/path/to/session-dir",
  "client_cwd": "/project",
  "launch_cwd": "/project",
  "runtime_instance_id": "rtinst_xxx",
  "start_command": "pi --approve -e /path/to/pontia/clients/pi",
  "tmux": {
    "socket_path": "/tmp/tmux-1000/default",
    "session_id": "$1",
    "session_name": "dev",
    "window_id": "@3",
    "window_index": 0,
    "pane_id": "%42",
    "pane_index": 1
  }
}
```

响应示例：

```json
{
  "session": {
    "session_id": "sess_xxx"
  },
  "runtime": {
    "runtime_instance_id": "rtinst_xxx",
    "internal_event_url": "http://127.0.0.1:8080/internal/v1/events",
    "current_turn_file": ".../current-turn.json",
    "capabilities": {
      "accept_task": true,
      "interrupt": true,
      "stream_output": true
    }
  }
}
```

upsert key：

```text
client_type + client_session_key
```

如果未来发现 pi session key 不稳定，可增加 fallback，但第一版优先使用 pi 的真实 session id。

## 事件驱动 turn 上报

extension 尽量只根据 pi lifecycle 事件上报事实。

| pi event | pontia event |
| --- | --- |
| `session_start` | upsert binding；然后 `session.ready` |
| `agent_start` | `turn.created`（必要时）+ `turn.started` |
| `message_update` | `session.message_updated` |
| `message_end` | `session.message_updated` append/final |
| `agent_end` | `turn.output` + `turn.completed` 或 `turn.failed` |
| context usage observation | `session.context_usage_updated` |

### Web UI 输入与 current-turn context

Web UI submit 时，pontia 仍需做两件事：

1. 校验 capability，例如 `accept_task`。
2. 写 `current-turn.json` 并将输入 dispatch 到 runtime。

之后不应由 Web dispatch 逻辑直接伪造完成状态。turn started/completed/failed 应由 extension 事件确认。

### TUI 输入与 turn id

TUI 用户直接输入时可能没有有效 current-turn context。extension 在 `agent_start` 时：

- 如果 current-turn context 可用且未结束，使用其中 turn id；
- 否则创建或请求新的 turn id；
- 上报 `turn.created` 和 `turn.started`。

## Web UI 行为

Web UI 只依据 `session.capabilities` 决定可用操作，不关心 session 是否 managed/attached。

| capability | UI 行为 |
| --- | --- |
| `accept_task = true` | 输入框启用 |
| `accept_task = false` | 输入框禁用，提示 “此 session 当前不可从 Web 写入” |
| `interrupt = true` | Stop/Interrupt 启用 |
| `interrupt = false` | Stop/Interrupt 禁用或隐藏 |
| `stream_output = true` | 展示实时消息更新 |

非 tmux session 的 Web 输入禁用只来自 `accept_task = false`，其他展示行为保持一致。

## Web UI dispatch 行为

对于 pi TUI session，Web UI dispatch 只有一种实现：通过 tmux 向 pi 所在 pane 模拟用户输入。

```text
Web UI input
  ↓
write current-turn context
  ↓
tmux paste/send-keys to tmux_socket_path + tmux_pane_id
  ↓
pi TUI receives input
  ↓
pi extension events
  ↓
pontia events/projections
```

如果 `tmux_socket_path` 和 `tmux_pane_id` 存在，则 `accept_task = true`；否则 `accept_task = false`，Web UI 输入框禁用。

控制命令示例：

```bash
tmux -S "$tmux_socket_path" send-keys -t "$tmux_pane_id" ...
```

不要依赖 `session:window.pane` index，因为 window/pane index 可变。

## restart 行为调整

Web UI restart 优先使用 `runtime_bindings.start_command`。

优先级：

```text
用户显式输入或 extension 上报的 start_command
> pontia config runtime.pi.tui_command
> agent client 默认命令
```

第一版建议 restart 仍由 capability 控制，而不是按入口分支。只有后端确认有可执行的 `start_command` 和可接受的重启目标时，才暴露 restart capability。

restart 后应更新：

- `runtime_instance_id`
- tmux fields
- `last_seen_at`
- `restart_count`
- `metadata.capabilities`

## 分阶段实现计划

每个 phase 应由一个独立 agent 完成。agent 只修改本 phase 明确列出的范围，并在交接说明中列出已完成项、验证命令、遗留风险和下一 phase 需要关注的接口契约。除 Phase 0 外，后续 phase 应基于前一 phase 已合并后的主线继续工作，避免多个 agent 同时修改同一批文件。

### Phase 0：基线确认与回归护栏

目标：在不改业务行为的前提下建立可执行的验收基线，降低后续多 agent 串行改动的集成风险。

范围：

1. 阅读并确认当前 `runtime_bindings`、pi runtime、pi extension、dispatch/control、External API session capability 的现状。
2. 补充或整理最小回归测试清单，优先覆盖当前已有行为：
   - pontia 启动 managed pi runtime 后能收到 `session.ready`；
   - Web submit 能写 current-turn context 并 dispatch 到 pi TUI；
   - pi extension 能按现有 env context 上报 turn lifecycle；
   - raw transcript `agent_bindings` 仍可解析 pi session file。
3. 不做 schema 或接口改动。

交付物：

- 一份简短交接说明，标明当前关键代码路径和后续 phase 需要保护的测试。
- 如果发现现有测试缺口，可以只新增不改变语义的测试。

验证：

- `cargo test`
- `pnpm --dir clients/pi test`
- `pnpm --dir clients/pi typecheck`

### Phase 1：runtime_bindings schema 迁移与兼容读写

目标：落地新 schema，移除 `runtime_ref` 的业务语义，同时保持当前 managed pi runtime 创建和现有测试可运行。

范围：

1. 追加新 migration，重建 `runtime_bindings` 表：
   - 新增 `runtime_instance_id`、`start_command`、`launch_cwd`、`last_seen_at`、`tmux_socket_path`、`tmux_pane_id`；
   - 旧 `runtime_ref` 不再作为顶层字段；
   - 如需诊断，将旧值写入 `metadata.legacy_runtime_ref`；
   - tmux session/window/pane index 等辅助信息放入 `metadata.tmux`。
2. 更新后端所有 `runtime_bindings` SQL 读写：
   - managed runtime 创建时写入新字段；
   - 查询 capability 仍从 `metadata.capabilities` enrich；
   - 禁止新增对 `runtime_ref` 的依赖。
3. 为迁移和新字段持久化补测试。

交付物：

- 新 migration。
- 后端持久化和查询代码适配。
- `runtime_ref` 仅可出现在历史迁移或 legacy diagnostic 迁移逻辑中。

验证：

- `cargo fmt --check`
- `cargo test`

### Phase 2：后端 runtime binding upsert API

目标：提供 pi extension 可调用的幂等绑定入口，并由后端统一创建/复用 pontia session。

范围：

1. 新增 internal API：`POST /internal/v1/runtime-bindings/upsert`。
2. 新增 upsert service：
   - upsert key 为 `client_type + client_session_key`；
   - 首次 upsert 创建 workspace/session；
   - 重复 upsert 复用既有 session；
   - 更新 `runtime_instance_id`、`start_command`、`launch_cwd`、`last_seen_at`、`tmux_socket_path`、`tmux_pane_id` 和 `metadata`；
   - upsert `agent_bindings`，保留 raw transcript 能力。
3. 根据 tmux 信息计算 capability：
   - 有 `tmux_socket_path + tmux_pane_id`：`accept_task=true`、`interrupt=true`；
   - 无 tmux pane：`accept_task=false`、`interrupt=false`；
   - `stream_output`、`report_turn_started`、`report_turn_finished` 仍为 true。
4. 返回 session/runtime context：`session_id`、`runtime_instance_id`、`internal_event_url`、`current_turn_file`、`capabilities`。

交付物：

- Internal API route、request/response 类型、service。
- upsert 幂等测试、tmux/non-tmux capability 测试、agent binding 测试。

验证：

- `cargo fmt --check`
- `cargo test`

### Phase 3：Web dispatch 与 runtime control 改为 tmux pane binding

目标：让 pi TUI 的 Web submit/interrupt 只依赖 `tmux_socket_path + tmux_pane_id`，不再区分 managed/attached runtime 分支。

范围：

1. 调整 turn dispatch：
   - 写 current-turn context；
   - 使用 `tmux -S <tmux_socket_path>` 和 `-t <tmux_pane_id>` paste/send input；
   - 缺少 tmux pane 时返回 capability unavailable / runtime cannot accept tasks。
2. 调整 interrupt：
   - 使用同一 pane binding 发送 interrupt；
   - 缺少 tmux pane 时按 capability 不支持处理。
3. dispatch 不直接伪造 turn completion/failure；完成状态仍等待 pi extension lifecycle 事件确认。
4. restart 最小改动：优先读取 `runtime_bindings.start_command`，无值时 fallback 到 config/default。

交付物：

- tmux pane dispatch/control 实现。
- 有 pane 可 dispatch、无 pane 不可写、interrupt pane target、restart start_command 优先级测试。

验证：

- `cargo fmt --check`
- `cargo test`

### Phase 4：pi extension 自动 upsert 与统一 session context

目标：pi TUI 加载 extension 时自动绑定 pontia session；managed env 模式和 auto-attached 模式产出同一种 session/turn context。

范围：

1. 新增 attach/upsert 模块：
   - 发现 pontia server/API URL；
   - 采集 pi session 信息：`getSessionId()`、`getSessionFile()`、`getSessionDir()`、`getCwd()`；
   - 采集 `process.env.TMUX`、`process.env.TMUX_PANE`；
   - 在 tmux 中通过 `tmux display-message` 补全 session/window/pane metadata；
   - 调用 `/internal/v1/runtime-bindings/upsert`；
   - 保存 bound context，供后续 lifecycle events 使用。
2. `session_start`：
   - managed env 模式仍可直接 report `session.ready`；
   - 无 env 时尝试 auto-upsert；
   - upsert 成功后 report `session.ready`；
   - server 不可达时不阻塞 pi，并将当前 pi session 标记为 unbound。
3. `agent_start`：
   - 使用有效 current-turn context；
   - context 缺失/过期/已结束时，按 TUI 直接输入路径创建新 turn；
   - 校验 `session_id`、`runtime_instance_id`、`client_type`。
4. 后续 lifecycle events：
   - unbound session 静默忽略；
   - bound session 统一走现有 `turn.created` / `turn.started` / `turn.output` / `turn.completed` / `turn.failed` 上报。

交付物：

- pi extension attach/upsert 模块和 context 重构。
- managed env、auto-upsert tmux、auto-upsert non-tmux、server unavailable unbound、TUI direct turn 测试。

验证：

- `pnpm --dir clients/pi test`
- `pnpm --dir clients/pi typecheck`

### Phase 5：External API / Dashboard capability 收口

目标：确保 Web UI 只根据 session capabilities 决定可用操作，不硬编码 managed/attached 或 tmux/non-tmux 分支。

范围：

1. 检查 External session view：
   - capability 从 projection/binding metadata enrich；
   - 可补充暴露 binding diagnostics，但 UI 行为只看 capability。
2. 检查 Dashboard：
   - `accept_task=false` 时禁用输入框并提示“此 session 当前不可从 Web 写入”；
   - `interrupt=false` 时禁用或隐藏 Stop/Interrupt；
   - non-tmux session 的 turns/output/usage/transcript/artifacts 仍正常展示。
3. 不让 Web UI 读取 SQLite、runtime dirs、workspace files 或 internal API。

交付物：

- Dashboard capability gating 调整。
- tmux writable session 输入启用、non-tmux attached session 输入禁用但展示正常的 UI/API 测试。

验证：

- `pnpm --dir=apps/dashboard run check`
- `pnpm --dir=apps/dashboard run build`
- 必要时 `cargo test`

### Phase 6：端到端集成与清理

目标：串联前面所有 phase，清理旧分支/旧语义，确保设计目标完整落地。

范围：

1. 运行完整验证：
   - managed pi runtime；
   - 用户 tmux 中手动打开 pi；
   - 非 tmux 普通终端打开 pi；
   - Web submit、TUI direct input、interrupt、restart；
   - raw transcript/timeline/artifacts 展示。
2. 删除或重命名残留的 `runtime_ref` 业务概念、managed/attached dispatch 分支、过期测试 helper。
3. 更新 README 或开发说明中与新 upsert/绑定行为直接相关的用户可见内容；不要写入本地-only pontia notes。
4. 汇总风险：tmux pane 失效、server 不可达 unbound、restart 对用户手动 pane 的限制。

交付物：

- 清理提交。
- 端到端验证记录。
- 如有未解决项，写入后续 TODO/spec，而不是隐藏在代码注释中。

验证：

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `pnpm --dir clients/pi test`
- `pnpm --dir clients/pi typecheck`
- `pnpm --dir=apps/dashboard run check`
- `pnpm --dir=apps/dashboard run build`

## 测试计划

### 后端

- migration 能在旧数据库上成功追加字段。
- runtime binding upsert：
  - 首次创建 session；
  - 同一 `client_session_key` 再次 upsert 复用 session；
  - tmux fields 正确持久化；
  - non-tmux capability 为 `accept_task=false`。
- Web dispatch：
  - 有 `tmux_socket_path + tmux_pane_id` 时使用 pane id 注入输入；
  - 缺少 tmux pane 时返回不可写。
- restart：
  - 有 `start_command` 时优先使用；
  - 无 `start_command` 时 fallback config/default。

### pi extension

- managed env 下仍能正常上报 `session.ready`。
- 无 pontia env 时自动 upsert session。
- tmux 中能采集 `TMUX_PANE` 和 socket path。
- 非 tmux 中 upsert 后 capability 不可写。
- TUI 直接输入能产生 `turn.created` / `turn.started` / `turn.completed`。
- Web dispatch 输入最终由同一套 `agent_start` / `agent_end` 路径确认。

### Web UI

- tmux writable session 输入框启用。
- non-tmux attached session 输入框禁用，但 turns/output 正常展示。
- interrupt/restart 按 capability 显示或禁用。

## 边界情况处理

### 只安装 pi extension，但没有启动 pontia server

这是允许状态。pi extension 不应阻塞或影响 pi TUI 正常使用，但也不应在 server 后续启动后把一个已经运行过的 pi session 半路绑定进 pontia。否则 pontia 会得到一个缺失前序事件的 session，破坏事件流完整性。

处理方式：

1. `session_start` 时尝试发现/连接 pontia server。
2. 如果 server 不可达：
   - 不创建 pontia session；
   - 不上报 turn 事件；
   - 不后台重试绑定当前 pi session；
   - TUI 继续正常运行；
   - 可写入本地 hook log 诊断，但不弹 UI 打扰用户。
3. extension 将当前 pi session 标记为 `pontia_unbound_for_session`。
4. 该 pi session 后续所有 lifecycle events 都静默忽略，不进入 pontia。
5. 用户如果希望纳入 pontia，需要在 pontia server 已启动后新开一个 pi TUI session，或未来通过显式 raw transcript import 导入历史。

### pontia server 启动后，已有 pi TUI 不自动再绑定

如果 pi TUI 已经运行，pontia server 后启动：

- 当前 pi session 不自动 upsert pontia session；
- 后续新 turn 也不进入 pontia；
- extension 对该 session 保持静默丢弃；
- 不从 TUI 屏幕、runtime log 或部分 lifecycle events 推断历史状态；
- 只有新的 pi session 在 `session_start` 时成功连接 pontia server，才创建/复用 pontia session。

### 同一个 pi session 重复 upsert

重复 upsert 必须幂等。

- upsert key 使用 `client_type + client_session_key`。
- 已存在 binding 时返回已有 pontia session。
- 更新 `runtime_instance_id`、`last_seen_at`、`tmux_socket_path`、`tmux_pane_id`、`start_command` 等最新 runtime 信息。
- 不重复创建 session。

### pi TUI 退出、pane 失效或进程被强杀

用户调整 tmux 布局、移动 pane、`break-pane` / `join-pane` 等操作通常不会改变 `tmux_pane_id`。只要 pane 仍存活，`tmux_socket_path + tmux_pane_id` 仍可定位目标 pane。

真正需要处理的是 pi runtime 退出或控制面状态不一致：

- pi 正常退出；
- pi 所在 pane 被 kill；
- tmux server 重启或 socket 不存在；
- pane id 不存在；
- 用户退出 pi 后 pane 仍存在，但 pane 中已不是当前 pi runtime；
- extension 重启后上报新的 pane id。

处理方式：

- 优先通过 pi extension 的 session/process exit hook 上报 `session.exited` 或等价 runtime-exited 事件。
- 后端收到 exit 事实后，将 session 标记为 `exited`，并清除或失效 Web 可写 capability。
- 如果是强杀、tmux server 重启等导致 extension 无法上报 exit，后端在 Web dispatch / interrupt / liveness check 时发现 `tmux_socket_path + tmux_pane_id` 不可用，可以直接按 runtime 已退出处理：写入 session exited/error 事件，而不是降级成一个继续可观察的 readonly session。
- 这种情况不是正常 readonly attached session；它代表原 runtime 已经结束或不可确认，应终止该 pontia session 的活动状态。

### current-turn context 过期或不匹配

extension 在 `agent_start` 使用 current-turn context 前必须校验：

- `session_id` 与当前绑定的 pontia session 一致；
- `runtime_instance_id` 与当前 binding 一致；
- `client_type = "pi"`；
- turn 未被当前 extension 标记为 ended。

如果 context 不可用或已过期，则按 TUI 直接输入路径创建/请求新的 turn。

### 非 tmux session

非 tmux session 不可从 Web 写入，但仍是完整 pontia session。

- `tmux_socket_path = NULL`
- `tmux_pane_id = NULL`
- `capabilities.accept_task = false`
- `capabilities.stream_output = true`
- `capabilities.report_turn_started = true`
- `capabilities.report_turn_finished = true`

Web UI 输入框禁用；turn 展示、输出、usage、transcript 等保持一致。

## 风险与注意事项

1. **pontia server 不可达时不要阻塞 TUI**
   - extension 必须允许 pi 正常运行。

2. **tmux pane id 只在 pane 生命周期内有效**
   - 布局调整或移动 pane 通常不改变 pane id。
   - pane 被 kill、tmux server 重启、socket 丢失时，后端应按 runtime exited/error 处理，而不是继续保留可写状态。

3. **不要伪造完成状态**
   - turn completion/failure 必须来自 pi extension hook 事件。

4. **attached restart 谨慎处理**
   - 用户手动 tmux pane 不应默认被 pontia kill/restart。

5. **capability 是 UI 和 API 的唯一行为开关**
   - 不要在 UI 中硬编码 managed/attached 分叉。

## 结论

该设计将 pontia session 作为唯一权威控制状态，将 pi TUI 和 Web UI 统一为同一个 runtime 的不同控制入口。runtime binding 负责描述控制能力和传输方式；pi extension 负责通过生命周期事件上报事实；Web UI 只根据 capabilities 开启或禁用操作。

这样可以保持强一致性：无论用户从 TUI 还是 Web UI 进入，最终都落入同一套 session、turn、event 和 projection 模型。非 tmux session 仅因为缺少可控 transport 而 Web 不可写，其余观察与展示行为保持一致。
