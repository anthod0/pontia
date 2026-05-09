# 统一 Coding Agent Control Plane：产品愿景

**当前设计基线**：`llmparty` 已从 backend-only MVP 演进为可通过 Web Dashboard 和 HTTP External API 使用的 Coding Agent Control Plane。它以统一的 session / turn / event / artifact 模型管理不同 agent client，通过 tmux 承载长运行 runtime，并通过事件投影提供稳定的对外状态。

---

## 1. 产品定位

本项目的目标是提供一个统一的 Coding Agent Control Plane，将不同 coding agent client 收敛到一组稳定的控制和观测能力：

- 创建和管理 agent session
- 向 session 提交任务 / prompt / turn
- 查询 session、turn、event、artifact 状态
- 实时观察 session 事件和 turn 输出
- 控制 session 生命周期：interrupt、terminate、restart 等
- 通过统一事件模型聚合 runtime 和 agent client 回传的事实
- 让浏览器用户、脚本和上层 Orchestrator 都不需要直接理解具体 client 的运行细节

`llmparty` 的对外使用入口包括：

- Web Dashboard：面向人类用户的本地控制台
- HTTP External API：面向脚本、自动化系统和上层 Orchestrator 的稳定接口
- SSE event stream：面向 Dashboard 和 API consumer 的实时事件观察能力

---

## 2. 当前已支持能力

### 2.1 Control Plane 核心能力

- HTTP External API
- Web Dashboard
- API token 认证
- 创建、查询、终止、重启 session
- 向 session 提交 turn
- 查询 turn 历史和当前状态
- 查询 session / turn 事件
- 通过 SSE 订阅 session / turn 事件流
- session / turn 状态由领域事件投影得到
- 写操作支持 `Idempotency-Key` 幂等语义
- 稳定的 JSON 响应 envelope 和错误语义

### 2.2 Runtime 与 client 能力

- 使用 `tmux` 承载长运行 runtime
- runtime binding 作为辅助状态记录，不作为对外领域状态事实源
- 支持 `generic` client，用于验证通用 adapter contract
- 支持 `pi` client，包含 session 创建、turn dispatch、输出/完成/失败事实回传
- capability model 显式表达 client 支持的能力
- 不支持的能力返回明确降级错误，而不是伪造成功事件

### 2.3 Artifact 能力

- artifact metadata 查询
- artifact content 读取
- workspace artifact discovery
- artifact 与 session / turn 关联
- artifact discovery 不改变 session / turn 主状态
- External API 不直接暴露 runtime 内部路径作为权威状态来源

---

## 3. 当前不包含的能力

以下能力仍属于后续演进范围，不应被当前设计或 API consumer 假设为已存在：

- WebSocket 实时通道
- Approval / human-in-the-loop
- RBAC / OAuth / 多租户权限体系
- 多 turn 并发执行
- client-native session resume
- terminal preview / runtime diagnostic UI
- 进程池 / pre-warm pool
- 大规模分布式部署
- 复杂事件补洞、修复和 replay API
- artifact retention / compaction / 全文搜索

这些能力可以在当前稳定模型上继续扩展，但不应改变 session / turn / event / artifact 的核心抽象。

---

## 4. 核心使用流程

### 4.1 Web Dashboard 流程

```text
User
  -> open /dashboard
  -> enter External API token
  -> create/select session
  -> submit turn
  -> observe events and latest output through SSE
  -> browse artifacts
  -> interrupt/restart/terminate session when needed
```

Dashboard 只消费 External API 和 SSE event stream。它不直接读取 SQLite、tmux、runtime binding、client 日志或 workspace 文件作为权威状态来源。

### 4.2 HTTP API 编排流程

```text
External Orchestrator
  -> POST /external/v1/sessions
  -> POST /external/v1/sessions/{session_id}/inbox/messages
  -> GET  /external/v1/sessions/{session_id}/turns/{turn_id}
  -> GET  /external/v1/sessions/{session_id}/events
  -> GET  /external/v1/sessions/{session_id}/events/stream
  -> GET  /external/v1/sessions/{session_id}/artifacts
  -> GET  /external/v1/artifacts/{artifact_id}/content
  -> DELETE /external/v1/sessions/{session_id}
```

External API 是唯一对外控制与状态读取入口。调用方不直接读取 runtime backend、client 日志、工作目录或 client 内部状态。

---

## 5. 状态与事实来源

Control Plane 对外暴露的领域状态以自身投影为准。状态更新可来自多个内部来源：

- External API command processor
- Runtime Manager
- generic client adapter
- pi client adapter / hook
- agent client 回传事件
- filesystem / artifact adapter
- system monitor

其中 session / turn 等领域状态必须通过领域事件更新；runtime binding、artifact cursor、heartbeat timestamp、diagnostic metadata 等辅助状态可直接维护，不要求事件重放。

关键原则：

- 对外 session / turn 状态不以 tmux、日志或 client 内部状态为权威来源
- client-specific 字段不污染统一领域事件语义
- SSE 是实时观察优化，不替代 External API 的权威快照查询
- artifact 是可查询产物，不默认改变 session / turn 主状态
- capability model 用于表达能力差异和降级行为

---

## 6. 后续愿景

在当前 Control Plane、Dashboard、SSE 和 pi client 基线之上，后续可继续扩展：

- Human-in-the-loop approval
- 更丰富的 runtime diagnostics
- terminal preview 或受控的 runtime 观察界面
- Claude Code、Codex 等更多 client-specific adapter
- client-native session resume
- 多 session / 多 workspace 的更强管理体验
- 更强的权限、安全和审计体系
- WebSocket 或其他实时通道，如果 SSE 不再满足需求
- Process pool / pre-warm，用于降低 session 启动延迟

这些能力属于产品增强，应建立在当前统一 External API、事件模型和 projection 语义之上。
