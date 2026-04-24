# Control Plane 整体设计

## 1. 文档目标

本文定义 MVP 阶段 Control Plane 的顶层架构、边界、领域模型和主要数据流。

MVP 定位为：**backend-only 的可控型 agent control plane**。它通过 HTTP External API 提供完整 agent 编排能力；Web UI、WebSocket、Approval、具体 client 深度适配均不属于 MVP。

---

## 2. 系统边界

### 2.1 Control Plane 负责

- 通过 External API 接收上层 Orchestrator 的控制请求
- 创建、启动、终止和重启 agent session
- 向 session 提交 task / turn
- 维护 session / turn 的统一领域状态
- 接收 agent client 或 adapter 回传的领域事件
- 查询事件历史、turn 历史和 artifact
- 提供 interrupt / terminate 等基础运行时控制
- 为不同 client adapter 提供统一接入 contract

### 2.2 Control Plane 不负责

- agent client 内部推理和工具执行逻辑
- Web UI 与浏览器交互
- WebSocket / SSE 实时推送
- Approval / human-in-the-loop
- 具体 client 的 hook、日志、UI 操作细节
- 复杂多租户权限体系
- 分布式调度和资源编排

---

## 3. 设计原则

### 3.1 External API 是唯一对外控制与状态读取入口

上层 Orchestrator 只依赖 External API：

- 创建 session
- 提交 turn
- 查询状态
- 查询事件
- 读取 artifact
- 控制生命周期

调用方不直接读取 runtime backend、client 日志、workspace 文件或 client 内部状态。

### 3.2 领域状态事件化

session / turn 的领域状态必须由事件驱动更新。

事件可由多个内部来源产生：

- External API command processor
- Runtime Manager
- client adapter
- agent client
- system monitor

但只要会改变对外可见的 session / turn 状态，就必须进入统一 event store，并由 reducer 投影为当前状态。

### 3.3 辅助状态不强制事件溯源

以下状态属于实现或索引辅助，可直接维护：

- RuntimeBinding
- heartbeat timestamp
- artifact cursor
- idempotency key
- ingest warning
- schema migration
- internal lease / lock

这些状态不承诺通过事件流完整重放。

### 3.4 Client adapter 能力可分层

MVP 不绑定某个具体 client。Control Plane 定义通用接入 contract，具体 client 后续按能力实现。

最小能力：

- 接收任务输入
- 上报 turn started
- 上报 turn completed / failed

可选能力：

- streaming output
- heartbeat
- artifact discovery
- interrupt support
- richer lifecycle events

### 3.5 MVP 先 HTTP，后续再实时化

MVP 不做 WebSocket / SSE。调用方通过 HTTP 轮询查询状态。实时推送属于后续增强。

---

## 4. 总体架构

```text
External Orchestrator
        │
        ▼
External HTTP API
        │
        ▼
Application Layer
  ├─ Session Manager
  ├─ Turn Manager
  ├─ Runtime Control
  ├─ Event Ingest / Publisher
  ├─ State Reducer
  └─ Query Service
        │
        ▼
Storage
  ├─ Event Store
  ├─ Session / Turn Projections
  ├─ Runtime Metadata
  └─ Artifact Index
        │
        ▼
Runtime / Client Adapter Boundary
  ├─ AgentInputSink
  ├─ AgentEventSource
  └─ ArtifactSource
```

---

## 5. 核心子系统

### 5.1 External API

对外提供完整编排能力：

- session 管理
- turn 提交与查询
- event 查询
- artifact 查询
- runtime 控制

External API 不直接暴露内部 runtime 细节。

### 5.2 Session Manager

负责 session 生命周期的领域协调：创建、启动、终止、重启和状态查询。Session Manager 通过事件改变 session 状态。

### 5.3 Turn Manager

负责接收任务输入、创建 turn、向 adapter/runtime 下发输入，并根据事件更新 turn 状态。

### 5.4 Runtime Manager

负责运行时容器管理，例如启动 agent client、停止进程、执行 interrupt 等。Runtime Manager 是事件生产者和控制执行者，但不是 External API 的直接状态源。

### 5.5 Event Ingest / Publisher

统一接收来自 client adapter、runtime、External API command processor 的领域事件，写入 event store，并触发 reducer 更新投影。

### 5.6 State Reducer

根据事件更新 session / turn 投影。Reducer 应保持领域语义清晰，不包含具体 client 或 runtime 实现细节。

### 5.7 Artifact Adapter

负责发现和索引 client 产生的文件、日志、结果等 artifact。Artifact 是内容来源和索引系统，不默认作为 session / turn 主状态事实源。

---

## 6. MVP 领域模型

### 6.1 Session

代表一个可接收任务的 agent 会话。

建议状态：

- `created`
- `starting`
- `idle`
- `busy`
- `interrupted`
- `exited`
- `error`

### 6.2 Turn

代表一次任务提交与执行。

建议状态：

- `queued`
- `running`
- `completed`
- `failed`
- `interrupted`
- `cancelled`

### 6.3 Event

记录领域事实。MVP 重点覆盖 session / turn 生命周期事件。

### 6.4 RuntimeBinding

记录 session 与底层运行时实例的绑定关系。它是辅助状态，不参与领域事件重放承诺。

### 6.5 Artifact

记录外部可查询的内容索引，例如日志、输出文件、结果文件等。

---

## 7. 核心数据流

### 7.1 创建 Session

```text
POST /external/v1/sessions
  -> command processor 接受创建请求
  -> 发布 session.created / session.starting
  -> reducer 创建/更新 session projection
  -> Runtime Manager 启动 agent runtime
  -> Runtime 或 adapter 发布 session.started / session.ready
  -> reducer 将 session 投影为 idle
```

若请求包含 `initial_task`，则 session 创建请求被接受后继续发布首个 turn 的创建事件。Projection 不应先于领域事件成为事实来源。

### 7.2 提交 Turn

```text
POST /external/v1/sessions/{id}/turns
  -> command processor 接受 turn 请求
  -> 发布 turn.created / turn.queued
  -> reducer 创建/更新 turn projection
  -> 通过 AgentInputSink 下发任务
  -> adapter / agent 回传 turn.started
  -> adapter / agent 回传 turn.completed 或 turn.failed
  -> reducer 更新 turn 和 session 状态
```

### 7.3 中断 Turn / Session

```text
POST /external/v1/sessions/{id}/interrupt
  -> command processor 校验 session/turn 与 adapter capability
  -> 若支持 interrupt，发布 turn.interrupt_requested
  -> Runtime 或 adapter 执行 interrupt
  -> 成功后发布 turn.interrupted
```

MVP 的 External API 提供 interrupt 请求面，但 interrupt 是否强保证取决于 adapter/runtime capability。若当前 session 不支持 interrupt，API 应返回 capability unavailable，而不是伪造中断成功。

### 7.4 查询状态

External API 从 Control Plane projection 和 event store 查询，不直接访问 client 内部状态。

---

## 8. MVP 范围

### 8.1 必做

- HTTP External API
- session 创建、查询、终止、重启
- turn 提交、查询、中断
- session / turn 事件模型和 reducer
- event 查询
- artifact 元数据与内容查询
- generic client adapter contract
- 基础 runtime binding 抽象

### 8.2 延后

- Web UI
- WebSocket / SSE
- Approval / human-in-the-loop
- 具体 client 深度适配
- 进程池 / pre-warm
- 高级权限模型
- 分布式部署

---

## 9. 最终结论

MVP 的核心是一个可由 HTTP API 完整驱动的后端 Control Plane。它不以 Web UI 为中心，也不绑定具体 client。External API 是对外唯一控制面和状态读取面；内部通过统一领域事件维护 session / turn 状态，并允许不同 adapter 在后续阶段逐步接入。
