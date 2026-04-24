# Control Plane External API v1

## 1. 文档目标

本文定义 MVP 阶段 Control Plane 的 External HTTP API。

MVP 目标是提供一个功能齐全的 backend control plane，使上层 Orchestrator 可以通过 HTTP 完成完整 agent 编排：创建 session、提交任务、查询状态、读取事件和 artifact、执行中断或终止。

Web UI、WebSocket、SSE、Approval 不属于 MVP。

---

## 2. API 设计原则

### 2.1 External API 是唯一对外控制面

调用方只通过 External API 与 Control Plane 交互，不直接访问 runtime、client、日志目录或数据库。

### 2.2 API 面向编排，不暴露实现

External API 表达的是 session、turn、event、artifact 和 runtime control 等稳定概念，不暴露具体 client hook、runtime backend、进程识别、文件监听等实现细节。

### 2.3 HTTP-only MVP

MVP 只提供 HTTP request/response API。调用方通过轮询查询状态变化。

### 2.4 写操作应支持幂等

创建 session、提交 turn、interrupt、terminate 等写操作应具备幂等语义。具体幂等键传递和存储策略属于实现细化。

---

## 3. 认证模型

MVP 使用简单 API Key / Bearer Token 认证：

```http
Authorization: Bearer <api_key>
```

细粒度 RBAC、OAuth、多租户隔离属于后续阶段。

---

## 4. 通用响应形态

普通 JSON API 使用统一包装：

```json
{
  "data": {},
  "meta": {},
  "error": null
}
```

错误响应：

```json
{
  "data": null,
  "error": {
    "code": "invalid_request",
    "message": "..."
  }
}
```

Artifact content 读取是例外，可返回原始内容流或 range 内容，不强制 JSON 包装。

---

## 5. View Models

### 5.1 SessionView

```json
{
  "session_id": "sess_...",
  "client_type": "generic",
  "state": "idle",
  "current_turn_id": null,
  "workspace": "/repo",
  "capabilities": {
    "accept_task": true,
    "interrupt": false,
    "stream_output": false,
    "heartbeat": false,
    "artifact_sources": true
  },
  "created_at": "...",
  "updated_at": "...",
  "metadata": {}
}
```

MVP session state：

- `created`
- `starting`
- `idle`
- `busy`
- `interrupted`
- `exited`
- `error`

### 5.2 TurnView

```json
{
  "turn_id": "turn_...",
  "session_id": "sess_...",
  "state": "running",
  "input": {
    "summary": "...",
    "artifact_id": null
  },
  "output": {
    "summary": "...",
    "artifact_ids": []
  },
  "failure": null,
  "created_at": "...",
  "started_at": "...",
  "completed_at": null,
  "metadata": {}
}
```

MVP turn state：

- `queued`
- `running`
- `completed`
- `failed`
- `interrupted`
- `cancelled`

### 5.3 EventView

```json
{
  "event_id": "evt_...",
  "session_id": "sess_...",
  "turn_id": "turn_...",
  "source": "agent_adapter",
  "type": "turn.completed",
  "time": "...",
  "payload": {}
}
```

### 5.4 ArtifactView

```json
{
  "artifact_id": "art_...",
  "session_id": "sess_...",
  "turn_id": "turn_...",
  "kind": "log",
  "name": "agent.log",
  "size_bytes": 12345,
  "preview": "...",
  "created_at": "...",
  "metadata": {}
}
```

---

## 6. Session API

### 6.1 Create Session

```http
POST /external/v1/sessions
```

请求：

```json
{
  "client_type": "generic",
  "workspace": "/path/to/workspace",
  "metadata": {},
  "initial_task": {
    "input": "请完成这个任务",
    "metadata": {}
  }
}
```

说明：

- `initial_task` 可选。
- 如果提供，Control Plane 创建 session 后应创建首个 turn。
- 具体 client 如何启动、如何接收输入由 runtime / adapter 层处理。

响应：

```json
{
  "data": {
    "session": {},
    "initial_turn": {}
  }
}
```

若未提供 `initial_task`，`initial_turn` 为 `null`。

### 6.2 List Sessions

```http
GET /external/v1/sessions
```

支持按 state、client_type、created time 等条件过滤。分页方式可在实现阶段细化。

### 6.3 Get Session

```http
GET /external/v1/sessions/{session_id}
```

### 6.4 Delete / Terminate Session

```http
DELETE /external/v1/sessions/{session_id}
```

语义：请求 Control Plane 终止该 session 对应 runtime，并将最终事实通过领域事件反映到 session 状态。MVP 中 terminate 成功后 session 应最终进入 `exited`；若终止失败则进入或保持 `error`。终态 session 不再 restart，如需继续使用应创建新 session。

### 6.5 Restart Session

```http
POST /external/v1/sessions/{session_id}/restart
```

语义：请求重启非终态 session runtime。MVP 中 restart 不适用于 `exited` / `error` 终态 session；终态恢复通过创建新 session 表达。restart 成功路径通过新的 `session.starting` / `session.started` / `session.ready` 事件周期反映，历史 turn 保留。

---

## 7. Turn / Task API

### 7.1 Submit Turn

```http
POST /external/v1/sessions/{session_id}/turns
```

请求：

```json
{
  "input": "继续执行下一步",
  "metadata": {}
}
```

语义：

- 在指定 session 中提交一个新 turn。
- MVP 限制同一 session 同时最多一个活跃 turn。
- 如果 session 当前不可接收 turn，应返回状态冲突错误。
- MVP 中 `idle` 和 `interrupted` session 可以接收新 turn；`busy`、`starting`、`exited`、`error` 不可接收新 turn。
- 成功后 Control Plane 创建 turn，并通过 adapter/runtime 下发输入。

响应：

```json
{
  "data": {
    "turn_id": "turn_...",
    "session_id": "sess_...",
    "state": "queued"
  }
}
```

### 7.2 List Turns

```http
GET /external/v1/sessions/{session_id}/turns
```

### 7.3 Get Turn

```http
GET /external/v1/sessions/{session_id}/turns/{turn_id}
```

### 7.4 Interrupt Current Turn

```http
POST /external/v1/sessions/{session_id}/interrupt
```

语义：请求中断 session 当前活跃 turn。MVP 提供该请求面，但是否可执行取决于 session adapter/runtime capability；不支持时应返回 `capability_unavailable`，不得伪造中断成功。

### 7.5 Interrupt Specific Turn

```http
POST /external/v1/sessions/{session_id}/turns/{turn_id}/interrupt
```

语义：请求中断指定 turn。MVP 中如果指定 turn 不是当前活跃 turn，应返回状态冲突或已终态响应；如果 adapter/runtime 不支持 interrupt，应返回 `capability_unavailable`。

---

## 8. Event Query API

### 8.1 List Session Events

```http
GET /external/v1/sessions/{session_id}/events
```

返回指定 session 的领域事件历史。用于审计、调试和编排方判断细节状态。

### 8.2 List Turn Events

```http
GET /external/v1/sessions/{session_id}/turns/{turn_id}/events
```

返回指定 turn 相关事件。

---

## 9. Artifact API

### 9.1 List Session Artifacts

```http
GET /external/v1/sessions/{session_id}/artifacts
```

### 9.2 Get Artifact Metadata

```http
GET /external/v1/artifacts/{artifact_id}
```

### 9.3 Read Artifact Content

```http
GET /external/v1/artifacts/{artifact_id}/content
```

内容读取可以返回原始内容或 range 内容。调用方不应通过该接口读取未注册 artifact 之外的任意文件。

---

## 10. 编排流程示例

### 10.1 创建 session 并提交首个任务

```text
POST /external/v1/sessions { initial_task }
  -> returns session_id + initial_turn_id

GET /external/v1/sessions/{session_id}/turns/{turn_id}
  -> poll until completed / failed / interrupted

GET /external/v1/sessions/{session_id}/events
  -> inspect event history
```

### 10.2 已有 session 提交后续任务

```text
POST /external/v1/sessions/{session_id}/turns
  -> returns turn_id

GET /external/v1/sessions/{session_id}/turns/{turn_id}
  -> poll current state
```

### 10.3 中断任务

```text
POST /external/v1/sessions/{session_id}/interrupt
  -> returns accepted command state

GET /external/v1/sessions/{session_id}
  -> poll session state
```

---

## 11. 错误语义

| 场景 | 建议状态码 | 说明 |
|---|---|---|
| 未认证 | 401 | 缺少或无效 token |
| 无权限 | 403 | 当前 token 不允许该操作 |
| 请求不合法 | 400 | schema 或参数错误 |
| 资源不存在 | 404 | session / turn / artifact 不存在 |
| 状态冲突 | 409 | session 忙、turn 已终态等 |
| 能力不可用 | 422 或 501 | 当前 adapter/runtime 不支持该操作，例如 interrupt |
| 服务端错误 | 500 | 内部错误 |

---

## 12. MVP 不包含的 API

以下 API 不属于 MVP：

```http
GET /external/v1/approvals
POST /external/v1/approvals/{id}/approve
POST /external/v1/approvals/{id}/deny
GET /external/v1/ws
GET /external/v1/ws-events
GET /external/v1/events/stream
```

Approval、WebSocket、SSE 应在后续阶段独立设计。

---

## 13. MVP 范围

### 13.1 MVP 必做

- API Key 认证
- session 创建 / 查询 / 终止 / 重启
- turn 提交 / 查询 / 中断
- event 查询
- artifact 查询与内容读取
- 基本错误语义
- 写操作幂等语义

### 13.2 Post-MVP

- Web UI
- WebSocket / SSE
- Approval / human-in-the-loop
- RBAC / OAuth
- 复杂调度和并发 turn
- 具体 client adapter 扩展 API

---

## 14. 最终结论

External API v1 的 MVP 是完整 agent 编排后端接口，而不是只读查询接口。上层 Orchestrator 必须能够仅通过 HTTP API 创建 session、提交任务、查询状态、读取输出并控制生命周期。
