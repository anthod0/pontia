# 统一 Coding Agent Headless 接入层：产品愿景

**当前设计基线**：MVP 聚焦于一个 backend-only 的可控型 Control Plane。它通过 HTTP External API 为上层 Orchestrator 提供完整 agent 编排能力；Web UI、WebSocket、Approval、人类审批闭环、具体 client 深度适配均为后续阶段。

---

## 1. 产品定位

本项目的目标是提供一个统一的 Coding Agent Control Plane，将不同 agent client 收敛到一组稳定的后端能力：

- 创建和管理 agent session
- 向 session 提交任务 / prompt / turn
- 查询 session、turn、event、artifact 状态
- 控制 session 生命周期：interrupt、terminate、restart 等
- 通过统一事件模型聚合 agent client 回传的状态
- 让上层 Orchestrator 不需要直接理解具体 client 的运行细节

MVP 的核心用户不是浏览器用户，而是调用 External API 的自动化系统、脚本或 Orchestrator。

---

## 2. MVP 范围

### 2.1 MVP 必须支持

- Backend-only Control Plane
- HTTP External API
- 创建 session
- 提交任务 / turn
- 查询 session / turn / event / artifact
- interrupt / terminate / restart 等生命周期控制
- 领域状态事件化：session / turn 状态由事件投影得到
- 通用 client adapter contract：定义 Control Plane 与 agent client 的接入边界
- Artifact 元数据与内容读取能力

### 2.2 MVP 不包含

- Web UI
- WebSocket / SSE 实时推送
- Approval / human-in-the-loop
- RBAC / OAuth / 多租户权限体系
- 具体 client 的深度适配实现
- 进程池 / pre-warm pool
- 大规模分布式部署

这些能力可以作为后续阶段演进，但不应影响 MVP 的核心设计判断。

---

## 3. 核心使用流程

MVP 需要支持以下完整编排闭环：

```text
External Orchestrator
  -> POST /external/v1/sessions
  -> POST /external/v1/sessions/{session_id}/turns
  -> GET  /external/v1/sessions/{session_id}/turns/{turn_id}
  -> GET  /external/v1/sessions/{session_id}/events
  -> GET  /external/v1/artifacts/{artifact_id}/content
  -> DELETE /external/v1/sessions/{session_id}
```

External API 是唯一对外状态读取来源。调用方不直接读取 runtime backend、client 日志、工作目录或 client 内部状态。

---

## 4. 状态与事实来源

Control Plane 对外暴露的领域状态以自身投影为准。状态更新可来自多个内部来源：

- External API command processor
- Runtime Manager
- generic client adapter
- agent client 回传事件
- filesystem / artifact adapter
- system monitor

其中 session / turn 等领域状态必须通过领域事件更新；runtime binding、artifact cursor、heartbeat timestamp 等辅助状态可直接维护，不要求事件重放。

---

## 5. 后续愿景

MVP 完成后，可在稳定 External API 的基础上继续扩展：

- Web UI dashboard
- WebSocket / SSE 实时推送
- Human-in-the-loop approval
- Client-specific adapter，例如 pi、Claude Code、Codex 等
- 多 session 并发监控
- 更强的权限、安全和审计体系
- Process pool / pre-warm

这些能力属于产品增强，不应改变 MVP 的后端控制面核心定位。
