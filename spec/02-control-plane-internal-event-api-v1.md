# Control Plane Internal Event API v1

## 1. 文档目标

本文定义 MVP 阶段 Control Plane 内部领域事件接入协议。

Internal Event API 的目标是让 agent client、client adapter、Runtime Manager 或其他内部组件把 session / turn 事实上报给 Control Plane。Control Plane 将这些事实写入 event store，并通过 reducer 更新 External API 可读取的领域状态。

MVP 不包含 Approval 事件、WebSocket 推送或具体 client hook 细节。

---

## 2. 设计原则

### 2.1 事件只表达领域事实

Internal Event API 接收的是已经发生或被 Control Plane 接受的领域事实，例如：

- session started
- session ready
- turn started
- turn output
- turn completed
- turn failed
- session exited

它不直接暴露数据库结构、runtime 细节或具体 client 的 hook 格式。

### 2.2 事件来源可以不同，语义必须统一

事件来源可以是：

- agent client
- client adapter
- Runtime Manager
- External API command processor
- system monitor

但进入 event store 后应采用统一事件类型和字段语义。

### 2.3 领域状态由事件投影

session / turn 的对外状态必须由事件 reducer 更新。Internal Event API 是领域事件进入 Control Plane 的主要入口之一。

### 2.4 MVP 使用 HTTP

MVP 使用本机 HTTP 接口即可。Unix socket、批量 ingest、流式协议均为后续优化。

---

## 3. 协议概览

### 3.1 事件上报

```http
POST /internal/v1/events
Authorization: Bearer <session_or_adapter_token>
Content-Type: application/json
```

请求体：

```json
{
  "event_id": "evt_01H...",
  "session_id": "sess_01H...",
  "turn_id": "turn_01H...",
  "source": "agent_adapter",
  "client_type": "generic",
  "type": "turn.completed",
  "time": "2026-04-24T12:00:00Z",
  "seq": 42,
  "payload": {}
}
```

响应体：

```json
{
  "accepted": true,
  "event_id": "evt_01H...",
  "session_id": "sess_01H...",
  "turn_id": "turn_01H...",
  "state_version": 18
}
```

`turn_id` 在响应中返回，是为了支持由 Control Plane 预分配或确认 turn identity 的接入方式。

---

## 4. 字段语义

| 字段 | 说明 |
|---|---|
| `event_id` | 事件幂等 ID，同一事件重试必须保持不变 |
| `session_id` | Control Plane 分配的 session ID |
| `turn_id` | 与事件相关的 turn ID；session 级事件可为空 |
| `source` | 事件来源，如 `external_api`、`runtime_manager`、`agent_adapter`、`system_monitor` |
| `client_type` | client 类型，用于查询和诊断，不决定领域语义 |
| `type` | 统一事件类型 |
| `time` | 事件发生时间或上报时间 |
| `seq` | source 在 session 范围内提供的顺序号；具体顺序策略属于 ingest 设计 |
| `payload` | 事件附加数据，不存放大内容 |

---

## 5. MVP 事件类型

### 5.1 Session 事件

| 类型 | 含义 |
|---|---|
| `session.created` | Control Plane 创建 session |
| `session.starting` | session 开始启动 runtime/client |
| `session.started` | runtime/client 已启动 |
| `session.ready` | session 可接收任务 |
| `session.exited` | client 正常退出 |
| `session.error` | session 进入异常状态 |

### 5.2 Turn 事件

| 类型 | 含义 |
|---|---|
| `turn.created` | Control Plane 创建 turn |
| `turn.queued` | turn 已进入待执行状态 |
| `turn.started` | agent 开始处理 turn |
| `turn.output` | agent 产生一段输出摘要或引用 |
| `turn.completed` | turn 成功完成 |
| `turn.failed` | turn 失败 |
| `turn.interrupt_requested` | External API command processor 已接受中断请求 |
| `turn.interrupted` | runtime / adapter 确认 turn 已中断 |
| `turn.cancelled` | turn 在执行前或调度前取消 |

### 5.3 Post-MVP 事件

以下事件不属于 MVP：

- `approval.requested`
- `approval.resolved`
- WebSocket-specific event
- client-specific low-level hook event

这些能力应在对应阶段重新设计，不能影响 MVP 的核心事件模型。

---

## 6. 状态机原则

### 6.1 Session 状态

MVP session 状态：

- `created`
- `starting`
- `idle`
- `busy`
- `interrupted`
- `exited`
- `error`

典型转换：

```text
session.created   -> created
session.starting  -> starting
session.started   -> starting 或 idle（取决于是否需要 ready 信号）
session.ready     -> idle
turn.started      -> busy
turn.completed    -> idle
turn.failed       -> idle 或 error（取决于失败是否 session 级）
turn.interrupted  -> interrupted
session.ready     [interrupted] -> idle
turn.started      [interrupted] -> busy
session.exited    -> exited
session.error     -> error
```

### 6.2 Turn 状态

MVP turn 状态：

- `queued`
- `running`
- `completed`
- `failed`
- `interrupted`
- `cancelled`

典型转换：

```text
turn.created / turn.queued -> queued
turn.started               -> running
turn.output                -> running
turn.interrupt_requested   -> running
turn.completed             -> completed
turn.failed                -> failed
turn.interrupted           -> interrupted
turn.cancelled             -> cancelled
```

---

## 7. 可靠性要求

Internal Event API 应支持：

- 事件幂等：同一 `event_id` 重试不会产生重复状态变化
- 基本顺序检查：发现明显乱序或 gap 时记录 warning
- 原子投影：事件写入和领域投影更新保持一致
- payload 大小约束：大内容通过 artifact 引用，不直接放入 payload

具体数据库约束、gap 处理算法、索引设计属于实现细节，不在本文展开。

---

## 8. 错误响应原则

| 场景 | HTTP 状态 | 说明 |
|---|---|---|
| 认证失败 | 401 / 403 | token 无效或权限不足 |
| schema 不合法 | 400 | 字段缺失、事件类型不支持 |
| 状态冲突 | 409 | 事件与当前领域状态冲突 |
| 服务端错误 | 500 | 存储或内部处理失败，可重试 |

---

## 9. MVP 边界

### 9.1 MVP 包含

- HTTP event ingest
- session / turn 事件类型
- 幂等事件接收
- reducer 驱动的状态投影
- payload 中携带 artifact 引用

### 9.2 MVP 不包含

- Approval 事件
- 批量 ingest
- WebSocket / SSE
- 具体 client hook 协议
- 复杂 replay / gap repair API

---

## 10. 最终结论

Internal Event API 是 Control Plane 领域事实进入系统的统一入口。MVP 只覆盖 session / turn 生命周期事件，并服务于 External API 的完整后端编排能力。具体 client 如何产生这些事件属于 adapter 层后续设计。
