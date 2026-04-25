# MVP Milestones

本文档根据 `spec/` 下的产品与设计文档，编排 MVP 的实现顺序。它只描述阶段目标、依赖关系、交付物和验收门槛，不展开具体代码、数据库 schema 或框架实现细节。

当前工程技术方案：

- Backend：Rust
- 工程形态：单 binary crate，内部按 module 分层；MVP 阶段不拆 workspace
- Async runtime：Tokio
- HTTP transport：Axum；但领域层、存储层、应用服务层不得依赖 Axum 类型
- Storage：SQLite
- SQLite access：SQLx，使用 runtime query + migration；MVP 不启用必须依赖在线数据库的 compile-time query checking
- Migration：`migrations/` 目录管理 SQL migration；dev/test 启动时可自动执行 migration
- Serialization：Serde
- Time：`time` crate，统一 UTC
- ID：使用 UUID v7 生成底层 ID，对外包装为 `sess_...`、`turn_...`、`evt_...`、`art_...` 前缀形式
- Error：`thiserror` 定义领域 / 应用错误，HTTP 层统一映射为 External API error code
- Observability：`tracing` + `tracing-subscriber`
- Config：环境变量优先，`.env` 仅用于本地开发
- Test DB：集成测试使用临时 SQLite 文件并执行 migration，避免依赖开发者本机数据库状态

## 0. MVP 定位

MVP 是一个 **backend-only、HTTP-only 的 Coding Agent Control Plane**。

MVP 的完成标准是：上层 Orchestrator 可以只通过 External HTTP API 完成以下闭环：

1. 创建 session
2. 提交 initial task 或后续 turn
3. 查询 session / turn 当前状态
4. 查询 session / turn 事件历史
5. 查询 artifact metadata 并读取 artifact 内容
6. interrupt 当前或指定 turn
7. terminate / restart session

External API 是唯一对外控制面和状态读取面。调用方不直接读取 runtime、client 日志、workspace 文件、数据库或 client 内部状态。

## 1. MVP 明确不做

以下能力不进入 MVP milestone：

- Web UI
- WebSocket / SSE / 实时推送
- Approval / human-in-the-loop
- RBAC / OAuth / 多租户权限体系
- 具体 client 深度适配，例如 pi / Claude Code / Codex 的完整 adapter
- 进程池 / pre-warm pool
- 分布式调度与大规模资源编排
- 多 turn 并发
- 复杂 replay / gap repair API
- client-specific hook、日志解析、终端 UI 操作细节
- Axum 深度绑定或框架驱动的架构设计

## 2. 实现编排原则

- **先领域闭环，后运行时闭环**：先让 session / turn / event / projection 的状态模型成立，再接 runtime / adapter。
- **先稳定 API 契约，后丰富实现能力**：External API 和 Internal Event API 的语义应尽早固定。
- **所有对外状态来自 Control Plane 投影**：runtime 和 adapter 只能生产事件或辅助状态，不能成为 External API 的直接状态源。
- **写操作要幂等**：创建 session、提交 turn、interrupt、terminate、restart 都应有幂等语义。
- **能力驱动 adapter 行为**：interrupt、stream output、heartbeat、artifact source 等能力必须显式声明和降级。
- **MVP 保持单 session 单活跃 turn**：避免提前引入并发 turn 语义。
- **工程骨架先行**：项目初始化、Rust workspace / crate 边界、SQLite 基础接入、测试与格式化命令必须作为第一个 milestone 完成，避免后续 milestone 在无工程基线的状态下展开。
- **不让 Web 框架决定架构**：HTTP API 使用 Axum 承载，但领域、存储、应用服务和 adapter contract 必须保持 Axum 无关。

---

# - [x] Milestone 0：项目初始化与工程骨架

**状态：已完成**

完成时间：2026-04-24

完成摘要：已建立单 Rust backend crate 的工程骨架，接入 Axum HTTP transport、SQLx + SQLite、`migrations/`、环境变量配置、基础错误 / ID / UTC 时间边界、集成测试和 README 开发说明。当前结构采用 `src/application/`、`src/domain/`、`src/storage/`、`src/transport/http/`、`src/runtime/`、`src/adapters/` 分层，并预留 `apps/web/` 与 `docs/` 以承载后续完整产品。

完成验证：

- `cargo build`
- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 目标

建立 Rust + SQLite 的最小工程基线，让后续 milestone 可以在统一的项目结构、测试命令和本地运行方式上推进。

## 主要范围

- Rust 项目初始化
- 基础目录结构
- crate / module 边界初步划分
- SQLite 作为 MVP 存储后端的工程接入点
- 配置加载基线
- 测试、格式化、lint、构建命令基线
- 本地开发运行说明
- 建立 Axum transport 边界，但不让领域层、存储层、应用服务层依赖 Axum

## 依赖

无。此 milestone 是所有实现工作的工程前置。

## 交付物

- 可构建的 Rust backend 工程
- 明确的源码、测试、配置、文档目录
- SQLx + SQLite 存储接入的最小边界或占位模块
- 基础配置项：服务配置、数据库路径、认证 token 来源等
- 基础验证命令：build、test、format、lint
- 本地启动 / 开发说明
- 后续模块可依赖的错误类型、结果类型、时间 / ID 生成边界等基础设施
- `migrations/` 目录和最小 migration 执行路径

## 验收门槛

- 新 checkout 后可以按文档完成本地构建和测试
- SQLite 数据文件位置和配置方式明确
- 后续 milestone 不需要重新初始化项目结构
- 领域层和存储层不依赖 Axum 或其他 HTTP transport 类型
- 工程骨架没有引入 Web UI、WebSocket、Approval 或具体 client 深度适配

---

# - [x] Milestone 1：领域模型、事件与状态投影基线

**状态：已完成**

完成时间：2026-04-24

完成摘要：已建立 HTTP / SQLx 无关的 session、turn、event 领域模型和 reducer，新增 SQLite event store、session / turn projection、runtime binding 与 artifact index 辅助表，并提供 application 层 `EventIngestService` 完成事件持久化、幂等接收和投影更新。测试覆盖终态保护、单 session 单活跃 turn、辅助状态不改变主领域状态、事件持久化和 projection 更新。

完成验证：

- `cargo build`
- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 目标

建立 Control Plane 的核心领域语义，使 session / turn 的状态变化可以通过事件投影得到。

## 主要范围

- Session 领域对象
- Turn 领域对象
- Event 领域对象
- RuntimeBinding 辅助状态定位
- Artifact 辅助索引定位
- Session / Turn 状态机
- 单 session 单活跃 turn 约束
- Event Store 与 Projection 的职责边界
- Reducer 的领域语义

## 依赖

- Milestone 0

此 milestone 是后续 API、runtime、adapter 的领域基础。

## 交付物

- 可表达 MVP session 状态：`created`、`starting`、`idle`、`busy`、`interrupted`、`exited`、`error`
- 可表达 MVP turn 状态：`queued`、`running`、`completed`、`failed`、`interrupted`、`cancelled`
- 可接收并持久化 MVP session / turn 领域事件
- 可由事件更新 session / turn projection
- 明确区分领域状态与辅助状态

## 验收门槛

- 给定一组 session / turn 事件，可以得到正确的 session / turn 当前状态
- 终态 session 不会被迟到事件破坏对外状态
- 终态 turn 不会被后续事件错误重开
- 同一 session 同时最多一个活跃 turn
- artifact、runtime binding、heartbeat、cursor 等不会被误建模为主领域事实

---

# - [x] Milestone 2：Internal Event API v1

**状态：已完成**

完成时间：2026-04-24

完成摘要：已新增 `POST /internal/v1/events`，接入 Axum state 与 `EventIngestService`，支持 session / turn MVP 事件类型接收、schema 校验、payload 大小限制、RFC3339 时间解析、重复 `event_id` 幂等返回、基础 `seq` 乱序 / gap warning 记录，以及事件写入后 reducer 更新 projection。M2 暂不强制 Internal API 鉴权，按计划留待后续认证基线统一处理。

完成验证：

- `cargo build`
- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 目标

提供内部组件向 Control Plane 上报领域事实的统一入口。

## 主要范围

- `POST /internal/v1/events`
- 事件 schema 校验
- 事件来源字段
- session / turn 事件类型接收
- 幂等事件接收
- 基础顺序检查与 warning
- 事件写入后触发 reducer 更新 projection

## 依赖

- Milestone 1

## 交付物

- Internal Event API 可以接收以下 session 事件：
  - `session.created`
  - `session.starting`
  - `session.started`
  - `session.ready`
  - `session.exited`
  - `session.error`
- Internal Event API 可以接收以下 turn 事件：
  - `turn.created`
  - `turn.queued`
  - `turn.started`
  - `turn.output`
  - `turn.completed`
  - `turn.failed`
  - `turn.interrupt_requested`
  - `turn.interrupted`
  - `turn.cancelled`
- 同一 `event_id` 重试不会重复改变状态
- 返回事件接收结果和状态版本

## 验收门槛

- 非法 schema 返回明确错误
- 不支持的事件类型被拒绝
- 重复 `event_id` 保持幂等
- 事件写入和 projection 更新保持一致
- 大内容不能直接进入 event payload，只能通过 artifact 引用

---

# - [x] Milestone 3：External API 基础查询面

**状态：已完成**

完成时间：2026-04-24

完成摘要：已新增 External API 只读查询面，提供 Bearer Token 认证、统一 JSON 响应包装、session / turn / event / artifact metadata View Model，以及 M3 要求的 8 个 GET 端点。查询实现只读取 Control Plane projection、event store 与 artifact index，artifact metadata 响应不暴露 `source_ref` 等内部 source 细节。

完成验证：

- `cargo build`
- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 目标

建立 Orchestrator 可使用的只读状态查询面，为后续写操作闭环提供稳定 View Model。

## 主要范围

- API Key / Bearer Token 认证基线
- 统一 JSON 响应包装
- SessionView
- TurnView
- EventView
- ArtifactView
- session 查询
- turn 查询
- event 查询
- artifact metadata 查询入口

## 依赖

- Milestone 1
- Milestone 2

## 交付物

- `GET /external/v1/sessions`
- `GET /external/v1/sessions/{session_id}`
- `GET /external/v1/sessions/{session_id}/turns`
- `GET /external/v1/sessions/{session_id}/turns/{turn_id}`
- `GET /external/v1/sessions/{session_id}/events`
- `GET /external/v1/sessions/{session_id}/turns/{turn_id}/events`
- `GET /external/v1/sessions/{session_id}/artifacts`
- `GET /external/v1/artifacts/{artifact_id}`

## 验收门槛

- External API 只读取 Control Plane projection、event store 和 artifact index
- 响应中不暴露 runtime backend、client hook、日志路径等内部细节
- 未认证请求被拒绝
- 不存在的 session / turn / artifact 返回明确错误
- View Model 与 spec 中定义的字段语义一致

---

# - [x] Milestone 4：Session 创建与启动闭环

**状态：已完成**

完成时间：2026-04-24

完成摘要：已新增 `POST /external/v1/sessions`，通过 External API Bearer Token 认证接收 session 创建请求，生成 `session.created` / `session.starting` / `session.started` / `session.ready` 生命周期事件，并通过 reducer 将 session 投影推进到 `idle`。实现了最小 generic runtime manager 与 `runtime_bindings` capability metadata，SessionView 可返回 capability 信息；支持 `initial_task` 创建 queued 初始 turn；新增 `Idempotency-Key` 基线避免创建请求重试产生重复 session。

完成验证：

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 目标

让 Orchestrator 可以通过 External API 创建 session，并使 session 生命周期通过事件进入投影。

## 主要范围

- `POST /external/v1/sessions`
- session 创建命令处理
- `initial_task` 接收但不急于完成执行语义
- session lifecycle 事件发布
- Runtime Manager 抽象接入点
- RuntimeBinding 创建与维护
- generic adapter capability 暴露

## 依赖

- Milestone 1
- Milestone 2
- Milestone 3

## 交付物

- 创建 session 后产生 `session.created`
- 启动流程产生 `session.starting`
- runtime / adapter ready 后产生 `session.started` / `session.ready`
- SessionView 可返回 capability 信息
- 创建请求具备幂等语义
- 若请求包含 `initial_task`，能创建初始 turn 记录并返回身份信息

## 验收门槛

- Orchestrator 可以通过 `POST /external/v1/sessions` 获取 `session_id`
- session 状态最终可以从 `created` / `starting` 进入 `idle` 或 `error`
- session 状态变化全部由事件驱动
- RuntimeBinding 不成为 External API 的直接状态源
- 不支持的 client capability 被明确表达，而不是隐式假设可用

---

# - [x] Milestone 5：Turn 提交与执行闭环

**状态：已完成**

完成时间：2026-04-25

完成摘要：已新增 `POST /external/v1/sessions/{session_id}/turns`，通过 External API Bearer Token 认证接收 turn 提交请求，由 Control Plane 分配 `turn_id` 并生成 `turn.created` / `turn.queued` 事件。实现了最小 generic `AgentInput` 提交边界、`Idempotency-Key` 幂等重试、session 状态与 active turn 冲突校验；adapter/runtime 可继续通过 Internal Event API 上报 `turn.started` / `turn.output` / `turn.completed` / `turn.failed`，并由 reducer 推进 turn 与 session 投影。

完成验证：

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 目标

让 Orchestrator 可以向可用 session 提交 turn，并通过轮询看到 turn 从 queued 到终态的完整过程。

## 主要范围

- `POST /external/v1/sessions/{session_id}/turns`
- Turn Manager
- AgentInputSink 抽象
- turn identity 由 Control Plane 分配
- turn lifecycle 事件
- session busy / idle 联动投影
- 单活跃 turn 冲突处理

## 依赖

- Milestone 4

## 交付物

- 提交 turn 后产生 `turn.created` / `turn.queued`
- adapter / runtime 可通过 Internal Event API 上报：
  - `turn.started`
  - `turn.output`
  - `turn.completed`
  - `turn.failed`
- session 在 turn 运行时进入 `busy`
- turn 完成或失败后 session 回到合适状态
- 写操作具备幂等语义

## 验收门槛

- `idle` 和 `interrupted` session 可以接收新 turn
- `busy`、`starting`、`exited`、`error` session 不能接收新 turn
- 同一 session 不能同时存在多个活跃 turn
- turn 终态包括 `completed`、`failed`、`interrupted`、`cancelled`
- Orchestrator 可以只通过 External API 查询 turn 执行结果

---

# - [x] Milestone 6：Runtime 生命周期控制

**状态：已完成**

完成时间：2026-04-25

完成摘要：已新增 External API runtime 生命周期控制端点：session 当前 turn interrupt、指定 turn interrupt、session terminate、session restart。generic runtime 保持 `interrupt: false`，interrupt 请求按 capability 降级返回 `capability_unavailable` 且不伪造 `turn.interrupt_requested`；terminate 通过 `session.exited` 推进终态；restart 对非终态 session 产生新的 `session.starting` / `session.started` / `session.ready` 周期；控制写操作接入 `Idempotency-Key` 幂等响应。

完成验证：

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 目标

补齐 session / turn 的基础控制能力：interrupt、terminate、restart。

## 主要范围

- interrupt 当前 turn
- interrupt 指定 turn
- terminate session
- restart 非终态 session
- Runtime Manager 控制动作抽象
- capability unavailable 降级语义
- 控制命令事件化

## 依赖

- Milestone 4
- Milestone 5

## 交付物

- `POST /external/v1/sessions/{session_id}/interrupt`
- `POST /external/v1/sessions/{session_id}/turns/{turn_id}/interrupt`
- `DELETE /external/v1/sessions/{session_id}`
- `POST /external/v1/sessions/{session_id}/restart`
- interrupt 被接受后产生 `turn.interrupt_requested`
- interrupt 生效后产生 `turn.interrupted`
- terminate 最终通过 `session.exited` 或 `session.error` 反映状态
- restart 通过新的 `session.starting` / `session.started` / `session.ready` 周期反映状态

## 验收门槛

- 不支持 interrupt 的 adapter/runtime 返回 `capability_unavailable`，不得伪造成功
- 指定 turn 非当前活跃 turn 时返回状态冲突或已终态响应
- `exited` / `error` 终态 session 不允许 restart
- terminate 后 session 最终进入终态
- 所有控制写操作具备幂等语义

---

# - [x] Milestone 7：Artifact 查询与内容读取闭环

**状态：已完成**

完成时间：2026-04-25

完成摘要：已补齐 `GET /external/v1/artifacts/{artifact_id}/content`，通过 External API Bearer Token 认证从 artifact index 中已登记的 `source_ref` 读取内容。内容读取限定为已注册 artifact 的 `file://` 绝对路径来源，不提供任意路径读取能力；metadata 查询继续隐藏内部 source 细节；读取时校验 `size_bytes` 与实际内容大小一致，未注册或不支持的 source 返回明确错误。没有 artifact source 的 session 仍可返回空 artifact 列表且不影响 turn 闭环。

完成验证：

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

## 目标

让 Orchestrator 可以通过 External API 统一发现和读取 agent 产生的内容，而不依赖 client 文件结构。

## 主要范围

- ArtifactSource 抽象
- Artifact index
- Artifact metadata 查询
- Artifact content 读取
- session / turn 与 artifact 关联
- 大内容读取边界
- artifact 不作为主状态事实源

## 依赖

- Milestone 3
- Milestone 5

## 交付物

- `GET /external/v1/sessions/{session_id}/artifacts`
- `GET /external/v1/artifacts/{artifact_id}`
- `GET /external/v1/artifacts/{artifact_id}/content`
- ArtifactView 可表达：kind、name、size、preview、session、turn、metadata
- event payload 可以引用 artifact metadata
- artifact content 读取受已注册 source 限制

## 验收门槛

- External API 不能读取未注册 artifact source 之外的任意文件
- 大内容不会写入 event payload
- artifact 发现或更新不会默认改变 session / turn 状态
- artifact metadata 与 content 读取结果一致
- 没有 artifact source 的 adapter 可以返回空列表而不影响 turn 闭环

---

# - [ ] Milestone 8：Generic Client Adapter Contract 验证

## 目标

验证 MVP 不绑定具体 client，Control Plane 只依赖通用 adapter contract 即可完成编排闭环。

## 主要范围

- AgentInputSink
- AgentEventSource
- ArtifactSourceProvider
- Capability Model
- Control Plane 分配 session_id / turn_id
- adapter 能力不足时的显式降级
- 一个 generic / test adapter 级别的端到端验证替身

## 依赖

- Milestone 5
- Milestone 6
- Milestone 7

## 交付物

- adapter 能声明以下 MVP capability：
  - `accept_task`
  - `report_turn_started`
  - `report_turn_finished`
  - `interrupt`
  - `stream_output`
  - `heartbeat`
  - `artifact_sources`
- Control Plane 可以把 turn input 下发给 adapter
- adapter 可以通过 Internal Event API 回传 turn / session 事件
- adapter 可以注册 artifact source
- capability 缺失时 External API 行为明确

## 验收门槛

- 不需要 pi / Claude Code / Codex 深度适配也能完成 MVP 编排验证
- turn identity 始终由 Control Plane 分配
- adapter 不伪造无法确认的领域事实
- client-specific 原始字段不会污染统一领域语义
- interrupt / streaming / artifact source 等能力可以按 capability 独立开关

---

# - [ ] Milestone 9：MVP 端到端编排验收

## 目标

把前面能力组合成一个完整、可重复验证的 backend-only Control Plane MVP。

## 主要范围

- 端到端 Orchestrator 流程
- API 错误语义一致性
- 幂等语义一致性
- 状态投影一致性
- 基础可观测性与诊断
- MVP 文档与示例请求整理

## 依赖

- Milestone 0 至 Milestone 8

## 交付物

- 完整编排流程可运行：
  1. `POST /external/v1/sessions`
  2. `POST /external/v1/sessions/{session_id}/turns`
  3. `GET /external/v1/sessions/{session_id}/turns/{turn_id}`
  4. `GET /external/v1/sessions/{session_id}/events`
  5. `GET /external/v1/sessions/{session_id}/artifacts`
  6. `GET /external/v1/artifacts/{artifact_id}/content`
  7. `POST /external/v1/sessions/{session_id}/interrupt`
  8. `DELETE /external/v1/sessions/{session_id}`
- 主要错误场景有稳定响应：
  - 401 / 403 authentication failure
  - 400 invalid request
  - 404 resource not found
  - 409 state conflict
  - 422 或 501 capability unavailable
  - 500 internal error
- MVP 使用说明覆盖 External API、Internal Event API 和 generic adapter contract

## 验收门槛

- 上层 Orchestrator 不需要理解 runtime backend 或 client 内部状态
- 所有 session / turn 对外状态都能从事件和 projection 解释
- 写操作重试不会创建重复 session、turn 或状态变化
- backend-only HTTP polling 模式足以完成编排闭环
- Web UI、WebSocket、Approval 等 Post-MVP 能力没有混入 MVP 交付

---

# 3. 推荐执行顺序总览

```text
Milestone 0  项目初始化与工程骨架
     ↓
Milestone 1  领域模型、事件与状态投影基线
     ↓
Milestone 2  Internal Event API v1
     ↓
Milestone 3  External API 基础查询面
     ↓
Milestone 4  Session 创建与启动闭环
     ↓
Milestone 5  Turn 提交与执行闭环
     ↓
Milestone 6  Runtime 生命周期控制
     ↓
Milestone 7  Artifact 查询与内容读取闭环
     ↓
Milestone 8  Generic Client Adapter Contract 验证
     ↓
Milestone 9  MVP 端到端编排验收
```

其中 Milestone 6 和 Milestone 7 在 Milestone 5 之后可部分并行；Milestone 8 需要依赖前面 runtime、turn、artifact 的最小能力完成后再进行整体 contract 验证。

# 4. MVP 完成定义

MVP 完成时，应满足：

- Control Plane 是一个可独立运行的 Rust backend 服务
- SQLite 是 MVP 的状态存储后端
- External API 能完整驱动 agent session 生命周期
- Internal Event API 能接收 runtime / adapter / client 回传的领域事实
- session / turn 状态由事件投影得到
- artifact 可通过 Control Plane 查询和读取
- runtime 与 adapter 细节被隔离在内部边界之后
- generic adapter contract 足以支撑后续 pi / Claude Code / Codex 等具体 client 接入
- Post-MVP 能力没有成为 MVP 的前置依赖
