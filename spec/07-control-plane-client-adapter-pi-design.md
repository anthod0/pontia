# Control Plane Client Adapter 设计（MVP 通用接入）

## 1. 文档目标

本文原用于描述 pi client adapter。根据当前 MVP 边界，Control Plane 不在 MVP 中绑定具体 client，因此本文调整为 **通用 client adapter contract** 设计。

pi、Claude Code、Codex 等具体 client 的 hook、日志、UI 操作、approval 回传、turn 识别等细节均为后续 adapter 实现设计，不应影响 MVP 顶层判断。

---

## 2. MVP Adapter 目标

MVP 的 adapter 层只定义 Control Plane 与 agent client 之间的通用边界：

- Control Plane 如何向 agent session 下发 turn input
- agent / adapter 如何向 Control Plane 回传 session / turn 事件
- adapter 如何暴露 artifact source
- adapter 如何声明自身能力

不定义：

- 某个 client 的 hook 名称
- 某个 client 的日志路径
- 具体输入注入细节
- approval 阻塞或回传
- client-specific idle 判断
- client-specific token 注入方式

---

## 3. Adapter 抽象

### 3.1 AgentInputSink

Control Plane 向 agent 下发输入的抽象能力。

```text
submit_turn(session_id, turn_id, input)
interrupt(session_id, optional turn_id)
terminate(session_id)
```

具体实现可以是：

- stdin / process input
- terminal input
- HTTP callback
- message queue
- file drop
- custom client protocol

MVP 顶层不绑定具体方式。

### 3.2 AgentEventSource

agent 或 adapter 向 Control Plane 回传事件的抽象能力。

推荐通过 Internal Event API 上报：

```http
POST /internal/v1/events
```

MVP 事件包括：

- `session.started`
- `session.ready`
- `session.exited`
- `session.error`
- `turn.started`
- `turn.output`
- `turn.completed`
- `turn.failed`
- `turn.interrupted`

### 3.3 ArtifactSourceProvider

adapter 可以注册 artifact source，使 Control Plane 能索引和读取 client 产生的内容。

artifact source 不默认改变 session / turn 状态。

---

## 4. Capability Model

具体 client 能力可能不完整，因此 adapter 应声明 capability。External API 和上层 Orchestrator 可以据此判断可用能力。

MVP capability：

| 能力 | 含义 | MVP 要求 |
|---|---|---|
| `accept_task` | 能接收 Control Plane 下发的 turn input | 必需 |
| `report_turn_started` | 能报告 turn 开始 | 必需 |
| `report_turn_finished` | 能报告 turn completed / failed | 必需 |
| `interrupt` | 能响应 interrupt | 可选 |
| `stream_output` | 能报告中间输出 | 可选 |
| `heartbeat` | 能周期性报告健康 | 可选 |
| `artifact_sources` | 能提供 artifact source | 可选 |

Post-MVP capability：

| 能力 | 说明 |
|---|---|
| `approval_observable` | 能观察 client 请求审批 |
| `approval_enforced` | 能把审批结果传回 client 并影响执行 |
| `rich_tool_events` | 能上报工具级事件 |
| `native_session_resume` | 能恢复 client 原生 session |

Approval 不属于 MVP，因此不要求 adapter 支持。

---

## 5. Turn Identity

MVP 中 turn identity 由 Control Plane 分配。External API 创建 turn 后，adapter 接收已分配的 `turn_id`，并在后续事件中使用同一 `turn_id`。

这样可以避免把 turn identity 生成规则绑定到具体 client。

---

## 6. 事件回传原则

adapter 回传事件时应遵循：

- 使用 Control Plane 分配的 `session_id` 和 `turn_id`
- 不把大内容放入 event payload
- 大输出通过 artifact 引用
- client-specific 原始字段放入 payload 的诊断区域，不改变统一事件语义
- 若无法确认某个事实，不应伪造确定性事件

---

## 7. 错误与降级

adapter 能力不足时，应显式降级：

- 不支持 interrupt：External API 返回 `capability_unavailable` / unsupported，不接受为成功中断
- 不支持 streaming output：只上报 completed / failed
- 不支持 heartbeat：Control Plane 仅依赖 runtime lifecycle observation
- 不支持 artifact source：artifact 列表为空

不应在顶层设计中假设所有 client 都支持完整生命周期、流式输出或审批控制。

---

## 8. pi 适配说明（Post-MVP）

pi 的具体接入属于 Post-MVP adapter 实现设计，应在确认 pi 能力后单独细化：

- 是否有稳定 hook
- hook 能提供哪些上下文
- 如何接收 turn input
- 如何判断 turn started / completed / failed
- 是否支持 interrupt
- artifact / log 路径如何发现
- 是否可能支持 approval

在这些问题确认前，pi 不应作为 MVP 核心架构的前置假设。

---

## 9. MVP 范围

### 9.1 MVP 必做

- 通用 AgentInputSink 抽象
- 通用 AgentEventSource 抽象
- capability model
- Control Plane 分配 session_id / turn_id
- adapter 通过统一事件回传状态

### 9.2 Post-MVP

- pi adapter 具体实现
- client-specific hook helper
- approval 回传
- idle 识别
- fallback queue replay
- client-native session resume

---

## 10. 最终结论

MVP 的 client adapter 设计应保持通用和能力驱动。Control Plane 定义稳定的输入、事件和 artifact 边界；具体 client 如何接入、先接入哪个 client、如何处理 hook 和日志，均放到后续 adapter 实现阶段。
