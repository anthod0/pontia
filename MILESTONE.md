# Post-MVP Milestones

本文档描述 `llmparty` 在 MVP 完成后的推荐演进路线，只保留后续阶段的目标、顺序、交付物和验收门槛。

当前基线：

- MVP backend-only HTTP Control Plane 已完成。
- External API 已支持 session / turn / event / artifact 查询与基础写操作。
- Internal Event API v1 已支持 runtime / adapter 回传领域事实。
- session / turn 对外状态由 event store 和 projection 推导。
- generic adapter contract 已验证。
- MVP 端到端 HTTP polling 验收已通过。

Post-MVP 的核心目标是：从 generic / test adapter 闭环推进到真实 coding agent client 可编排、可运行、可观测、可展示。

---

## Post-MVP 总体原则

- **先接真实 client，再扩展体验层**：优先证明 Control Plane 可以驱动真实 coding agent，而不是先做 UI 或复杂实时层。
- **先正确边界，后能力完整**：pi adapter 第一阶段只建立 `client_type = "pi"`、runtime binding、capability 与 queued turn 语义；真实 turn dispatch 必须通过后续 adapter bridge 完成。
- **保持统一领域模型**：pi / Claude Code / Codex 等 client-specific 状态不得污染 session / turn / event 的统一语义。
- **所有对外状态仍来自 Control Plane**：External API 不直接读取 pi 内部状态、日志文件或 workspace 文件作为权威状态。
- **能力显式声明和降级**：interrupt、streaming、heartbeat、artifact discovery 等能力必须通过 capability model 表达。
- **真实 runtime 行为逐步增强**：先能启动 / 绑定 / 下发任务，再补充进程监督、恢复、日志、心跳和资源管理。
- **继续保持 backend-first**：Web UI、WebSocket / SSE、dashboard 等体验层在真实 adapter 闭环之后推进。

---

# - [x] Milestone 0：pi Client Adapter 最小闭环

**状态：已完成**

## 目标

接入真实 `pi` coding agent client，让上层 Orchestrator 可以继续只通过 External HTTP API 完成最小编排闭环：创建 session、提交 turn、轮询状态、查看事件、读取 artifact。

该阶段不追求 pi adapter 的所有高级能力，只验证 Control Plane 到真实 client 的最小可用链路。

## 主要范围

- pi adapter 边界定义
- pi runtime 启动或绑定策略
- session 创建时建立 pi runtime binding
- pi turn 提交后的 queued 语义
- pi capability model 映射到 External API SessionView
- 集成测试或可重复本地验收脚本

不包含：真实 pi turn dispatch、pi 执行事件桥接、pi artifact discovery。这些能力由 Milestone 1.5 / Milestone 2 补齐。

## 依赖

- 已完成 MVP backend-only HTTP Control Plane 基线
- generic adapter contract 已存在
- 当前 External API / Internal Event API 契约保持兼容

## 交付物

- [x] 一个 pi adapter 模块或边界实现
- [x] session 创建时可以选择或配置 `client_type = "pi"`
- [x] Control Plane 分配的 `session_id` / `turn_id` 能保留在 queued pi turn 中
- [x] External API 提交 pi turn 后产生 `turn.created` / `turn.queued`
- [x] pi turn 不通过临时 subprocess 或其他非 runtime-authority 路径执行
- [x] README 或独立文档说明当前 pi adapter 验收流程

## 验收门槛

- [x] Orchestrator 不需要理解 pi 的内部状态或文件结构
- [x] `client_type = "generic"` 的既有测试和行为不回退
- [x] `client_type = "pi"` 可以完成最小 session + queued turn 闭环
- [x] pi-specific 字段只存在于 adapter / runtime 内部边界，不能污染统一领域事件语义
- [x] pi adapter 不能伪造无法确认的领域事实
- [x] 不支持的 capability 返回明确降级，而不是假装成功
- [x] 至少有一个可重复验证的端到端测试或本地验收脚本

---

# - [x] Milestone 1：Runtime Manager Hardening

**状态：已完成**

## 目标

将 runtime 从最小 binding 推进到可承载真实 agent client 的运行时管理层，补齐进程生命周期、工作区、日志、心跳和失败处理的基础能力。

## 主要范围

- runtime process supervisor
- workspace directory 管理
- runtime environment 注入
- stdout / stderr 捕获
- runtime heartbeat
- crash detection
- terminate / restart 的真实 runtime 行为
- runtime diagnostic metadata 内部记录
- runtime 失败映射到 session / turn 事件

## 依赖

- Milestone 0

## 交付物

- [x] RuntimeManager 支持真实进程启动 / 停止 / 重启
- [x] session 与 runtime binding 的生命周期一致性校验
- [x] runtime crash 能产生明确 session / turn 事件
- [x] terminate 能实际停止对应 runtime
- [x] restart 能创建新的 runtime 周期并通过事件反映状态变化
- [x] 运行日志可以用于内部诊断，但不直接成为 External API 状态源

## 验收门槛

- [x] runtime 进程异常退出不会让 session 永久停留在错误的非终态
- [x] terminate 后没有遗留对应 runtime 进程
- [x] restart 后 session 能回到可接受 turn 的状态或明确失败
- [x] runtime 细节不泄露到 External API View Model
- [x] generic 和 pi client 的 runtime 行为都有测试覆盖

---

# - [ ] Milestone 1.5：pi Adapter Bridge 与 Turn Dispatch

**状态：未开始**

## 目标

在不引入临时 subprocess shortcut 的前提下，补齐真实 pi turn 流转。Control Plane 只负责创建 turn、投递任务、接收 adapter 回传事实；turn 主状态仍由 event store / projection 推导。

## 主要范围

- pi runtime 内的 adapter bridge 启动策略
- Control Plane 到 pi runtime 的 turn dispatch 通道
- adapter bridge 到 Internal Event API 的事实回传
- `turn.started` / `turn.output` / `turn.completed` / `turn.failed` 事件映射
- turn 与 session runtime binding 的一致性校验
- dispatch 失败、runtime 不可用、adapter 回传异常的错误语义
- 真实 pi 执行能力的本地验收脚本或集成测试策略

## 依赖

- Milestone 0
- Milestone 1

## 交付物

- 一个明确的 pi adapter bridge 进程 / 协议 / 投递机制设计
- External API 提交 pi turn 后，任务能进入对应 pi runtime
- pi adapter bridge 通过 Internal Event API 回传确认过的领域事实
- queued pi turn 能基于真实 adapter 事件进入 started / completed / failed
- README 或独立文档说明真实 pi turn dispatch 的本地验收流程

## 验收门槛

- 不使用临时 subprocess 路径绕过 runtime / adapter bridge
- External API 不直接读取 pi 内部状态作为权威状态
- adapter 不能伪造未确认的 turn 事实
- runtime crash / dispatch failure 能产生明确 session / turn 事件
- generic client 行为不回退

---

# - [ ] Milestone 2：Artifact Discovery 与 Filesystem Adapter 增强

**状态：未开始**

## 目标

增强 artifact 发现、注册和读取能力，让真实 coding agent 产生的文件能稳定、安全地通过 Control Plane 暴露给 Orchestrator。

## 主要范围

- workspace artifact discovery
- allowlist / sandbox 路径限制
- artifact metadata 刷新
- artifact preview 生成
- artifact kind 推断
- 大文件读取限制
- 内容一致性校验
- artifact 与 session / turn 的关联策略

## 依赖

- Milestone 0
- Milestone 1 可部分并行

## 交付物

- 可配置 artifact root / workspace root
- 自动或半自动 artifact registration 流程
- artifact metadata 能表达文件类型、大小、更新时间、preview
- artifact content 读取继续限制在已注册 source 内
- 大文件有明确错误或分页 / 下载策略

## 验收门槛

- External API 不能读取未授权路径
- artifact discovery 不改变 session / turn 主状态
- artifact metadata 与实际 content 保持一致
- pi adapter 产生的常见文件可以被发现和读取
- 大文件不会进入 event payload

---

# - [ ] Milestone 3：External Event Stream API

**状态：未开始**

## 目标

在 HTTP polling 之外提供实时事件读取能力，改善上层 Orchestrator 和未来 Web UI 的体验。

## 主要范围

- SSE 或 WebSocket 技术选择
- session event stream
- turn event stream
- event cursor / resume
- reconnect 语义
- backpressure / buffer 策略
- 认证与错误语义

## 依赖

- Milestone 0
- Milestone 1 推荐先完成

## 交付物

- 一个稳定的 External Event Stream API
- 支持从指定 cursor 继续读取事件
- 连接断开后可以恢复，不需要读取 runtime 内部日志
- 与现有 event store / projection 语义一致
- 文档说明 polling 与 streaming 的关系

## 验收门槛

- polling API 继续可用且语义不变
- stream 事件顺序与 event store 一致
- 未认证请求被拒绝
- reconnect 不重复造成状态变化
- stream API 不引入新的领域状态源

---

# - [ ] Milestone 4：Minimal Web Dashboard

**状态：未开始**

## 目标

提供一个最小 Web 控制台，用于观察和操作 Control Plane，服务 demo、调试和人工排障场景。

## 主要范围

- session list
- session detail
- turn history
- event timeline
- artifact browser
- create session
- submit turn
- interrupt / terminate / restart 操作入口
- API token 配置方式

## 依赖

- Milestone 0
- Milestone 3 可选但推荐

## 交付物

- 最小可运行 Web UI
- 能通过现有 External API 完成基础操作
- 能展示 session / turn / event / artifact 状态
- 不直接读取数据库、runtime 或 workspace 文件

## 验收门槛

- Web UI 只是 External API 的消费者
- UI 不引入新的控制面语义
- 后端 API 仍可被非 UI Orchestrator 独立使用
- 常见错误能在 UI 中清晰展示

---

# - [ ] Milestone 5：Operational Readiness

**状态：未开始**

## 目标

补齐部署、诊断、兼容性和维护能力，让项目更适合长期运行或交给其他使用者部署。

## 主要范围

- OpenAPI spec
- API compatibility tests
- structured logging
- request id / correlation id
- metrics
- Dockerfile / container run guide
- config validation
- migration bootstrap polish
- CI workflow
- release checklist

## 依赖

- Milestone 0
- Milestone 1 推荐先完成

## 交付物

- OpenAPI 文档
- Docker 或等价部署说明
- CI 执行 build / test / fmt / clippy
- 结构化日志包含 request / session / turn 关联信息
- metrics 覆盖请求、事件、runtime、错误等基础维度
- 配置错误能在启动时明确失败

## 验收门槛

- 新环境可以按文档启动服务
- CI 能阻止格式、lint、测试回退
- 主要 API 兼容性有自动化保护
- 运行时错误可通过日志和 request id 定位
- 部署文档不要求阅读源码才能理解

---

# 推荐执行顺序

```text
Milestone 0  pi Client Adapter 最小闭环
      ↓
Milestone 1  Runtime Manager Hardening
      ↓
Milestone 2  Artifact Discovery 与 Filesystem Adapter 增强
      ↓
Milestone 3  External Event Stream API
      ↓
Milestone 4  Minimal Web Dashboard
      ↓
Milestone 5  Operational Readiness
```

Milestone 1 和 Milestone 2 可以在 Milestone 0 后部分并行。Milestone 3 和 Milestone 4 应避免早于真实 adapter 闭环，否则容易只服务 demo 而无法验证真实 agent 编排能力。

---

# Post-MVP 第一阶段完成定义

完成 Milestone 0 后，应满足：

- Control Plane 可以驱动至少一个真实 coding agent client：pi。
- Orchestrator 仍然只依赖 External API，不依赖 pi 内部状态。
- pi adapter 通过 Internal Event API 回传领域事实。
- session / turn 对外状态仍由 event projection 得到。
- artifact 可以通过 Control Plane 统一查询和读取。
- generic adapter contract 没有被 pi-specific 实现破坏。

完成 Milestone 0 至 Milestone 2 后，应满足：

- 真实 runtime 生命周期可控。
- pi client 的常见输出和 artifact 能稳定暴露给 Orchestrator。
- crash、terminate、restart、artifact discovery 等真实运行场景有明确行为。

完成 Milestone 0 至 Milestone 5 后，应满足：

- Control Plane 可真实运行、可实时观察、可通过 UI 演示、可部署和维护。
- 后续接入 Claude Code、Codex 或其他 coding agent client 时，可以复用 pi adapter 验证出的边界和 runtime 能力。
