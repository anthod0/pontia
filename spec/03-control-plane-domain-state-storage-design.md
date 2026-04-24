# Control Plane 领域模型与状态存储设计

## 1. 文档目标

本文定义 MVP 阶段 Control Plane 的领域模型、状态边界和存储职责。

MVP 只覆盖 backend control plane 的核心领域：session、turn、event、runtime binding、artifact。Approval、WebSocket、具体 client 深度适配不属于本文 MVP 范围。

本文保持在设计层面，不规定具体 SQL、索引、Rust struct 或迁移实现。那些细节应在实现计划阶段根据本文语义制定。

---

## 2. 存储职责划分

### 2.1 领域事件存储

Event Store 保存 session / turn 领域事实。它是领域状态重建和审计的基础。

### 2.2 状态投影存储

Session 和 Turn 的当前状态以 projection 形式存储，供 External API 快速查询。Projection 由 reducer 根据领域事件更新。

### 2.3 辅助状态存储

以下状态不要求通过事件重放恢复：

- runtime binding
- heartbeat timestamp
- artifact cursor
- idempotency key
- ingest warning
- schema migration
- internal lease / lock

它们服务于运行时、查询、幂等或可观测性，不构成核心领域事实。

### 2.4 Artifact 内容存储

大内容不写入领域事件 payload。日志、transcript、输出文件等内容保留在原生文件系统或 artifact storage 中，数据库只保存索引、metadata 和引用。

---

## 3. 领域对象

### 3.1 Session

Session 表示一个可接收任务的 agent 会话。

核心语义字段：

- session identity
- client type
- workspace reference
- current state
- current turn reference
- lifecycle timestamps
- metadata

MVP 状态：

- `created`
- `starting`
- `idle`
- `busy`
- `interrupted`
- `exited`
- `error`

其中 `interrupted` 表示上一活跃 turn 已被中断，session 本身未终止。MVP 中 `interrupted` 可以接收新的 turn；新 turn 开始后 session 回到 `busy`，或 adapter/runtime 明确 ready 后回到 `idle`。

### 3.2 Turn

Turn 表示一次任务提交和执行。

核心语义字段：

- turn identity
- session identity
- input summary or reference
- current state
- output summary or artifact references
- failure information
- lifecycle timestamps
- metadata

MVP 状态：

- `queued`
- `running`
- `completed`
- `failed`
- `interrupted`
- `cancelled`

### 3.3 Event

Event 表示已经发生的领域事实。

核心语义字段：

- event identity
- session identity
- optional turn identity
- event source
- event type
- event time
- source sequence
- payload

payload 应只保存小型结构化事实和 artifact 引用，不保存完整日志或大输出。

### 3.4 RuntimeBinding

RuntimeBinding 表示 session 与底层 runtime 实例之间的绑定关系。它用于执行控制和诊断，不是领域状态的唯一事实来源。

### 3.5 Artifact

Artifact 表示可由 External API 查询和读取的内容索引，例如日志、输出文件、transcript、结果文件等。

Artifact 不默认改变 session / turn 状态。只有当 adapter 明确产生领域事件时，artifact 相关事实才影响领域投影。

### 3.6 Approval（Post-MVP）

Approval 不属于 MVP。相关状态机、表结构和 API 应在 human-in-the-loop 阶段重新设计，避免提前影响 MVP 的领域模型。

---

## 4. 领域事件

MVP 事件类型分为 session 和 turn 两类。

### 4.1 Session 事件

- `session.created`
- `session.starting`
- `session.started`
- `session.ready`
- `session.exited`
- `session.error`

### 4.2 Turn 事件

- `turn.created`
- `turn.queued`
- `turn.started`
- `turn.output`
- `turn.completed`
- `turn.failed`
- `turn.interrupt_requested`
- `turn.interrupted`
- `turn.cancelled`

### 4.3 事件来源

事件可来自：

- External API command processor
- Runtime Manager
- client adapter
- agent client
- system monitor

进入 event store 后，事件语义不应暴露具体来源的实现细节。

---

## 5. 状态转换原则

### 5.1 Session 状态转换

```text
created   --session.starting--> starting
starting  --session.started/session.ready--> idle
idle      --turn.started--> busy
interrupted --turn.started--> busy
interrupted --session.ready--> idle
busy      --turn.completed--> idle
busy      --turn.failed--> idle 或 error
busy      --turn.interrupt_requested--> busy
busy      --turn.interrupted--> interrupted
any non-terminal --session.exited--> exited
any non-terminal --session.error--> error
```

终态：

- `exited`
- `error`

`DELETE /sessions/{id}` 的 terminate 语义应最终产生 `session.exited` 或 `session.error`。MVP 中 `exited` 和 `error` 为终态，终态 session 不再 restart；如果需要从终态恢复，应创建新 session。`restart` 仅适用于非终态 session，并通过新的 `session.starting` / `session.started` / `session.ready` 周期表达。

终态后的事件处理策略应在实现阶段明确为“拒绝”或“审计保留但不改变状态”。顶层要求是：不能让终态后的迟到事件破坏当前对外状态。

### 5.2 Turn 状态转换

```text
turn.created / turn.queued -> queued
queued  --turn.started--> running
running --turn.output--> running
running --turn.completed--> completed
running --turn.failed--> failed
running --turn.interrupt_requested--> running
running --turn.interrupted--> interrupted
queued  --turn.cancelled--> cancelled
```

终态：

- `completed`
- `failed`
- `interrupted`
- `cancelled`

### 5.3 单活跃 Turn 约束

MVP 应保持每个 session 同时最多一个活跃 turn。这样可以简化通用 adapter contract，并避免在具体 client 能力未确定前引入并发 turn 语义。

后续若需要同一 session 多 turn 并发，应作为独立设计议题处理。

---

## 6. 写入与投影原则

一次领域事件 ingest 应满足：

1. 验证事件来源和 schema
2. 进行幂等检查
3. 写入 event store
4. 由 reducer 更新 session / turn projection
5. 记录 warning 或诊断信息
6. 对调用方返回投影后的最新状态版本

事件写入与领域投影更新应保持原子一致。具体事务、锁、索引、gap 处理算法属于实现细节。

---

## 7. 查询模型

External API 查询只读取 Control Plane 的状态视图：

- session projection
- turn projection
- event store
- artifact index

它不直接读取 agent client 的内部状态，也不要求调用方理解 runtime binding。

---

## 8. 重放与修复

MVP 的领域投影应具备从 event store 重建 session / turn 状态的能力。这一能力只承诺覆盖 session / turn 领域状态，不覆盖 runtime binding、artifact cursor、heartbeat 等辅助状态。

Projection rebuild 可作为运维或测试能力，不一定在首个 MVP API 中暴露。

---

## 9. MVP 范围

### 9.1 MVP 必做

- session / turn 领域模型
- event store 语义
- session / turn projection
- reducer 状态转换
- 单 session 单活跃 turn
- artifact index 的领域边界
- runtime binding 的辅助状态定位

### 9.2 Post-MVP

- Approval / human-in-the-loop
- WebSocket / SSE
- 多 turn 并发
- 复杂事件补洞和 replay API
- 具体 client adapter schema
- 详细数据库 schema 和迁移策略

---

## 10. 最终结论

MVP 的领域模型应服务于可控型后端编排：External API 创建 session、提交 turn、查询状态；Control Plane 通过统一领域事件维护 session / turn 投影。本文刻意避免过早绑定数据库和 runtime 实现细节，以保证后续实现可以在清晰领域边界下展开。
