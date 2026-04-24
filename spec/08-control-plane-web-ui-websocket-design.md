# Control Plane Web UI & WebSocket 设计（Post-MVP）

## 1. 文档目标

本文定义 Web UI 与 WebSocket 的后续演进方向。

根据当前 MVP 决策，Control Plane MVP 只实现 backend-only HTTP External API，不实现 Web UI、WebSocket 或 SSE。因此本文内容不参与 MVP 范围，也不应影响 MVP 的 External API、领域模型和 adapter contract 判断。

---

## 2. Post-MVP 定位

Web UI 的目标是在 Control Plane 后端能力稳定后，为人类用户提供可视化操作入口：

- 查看 session 列表
- 查看 turn 历史
- 查看事件和 artifact
- 发起 interrupt / terminate / restart
- 后续处理 approval / human-in-the-loop
- 后续接收实时状态推送

WebSocket 的目标是在轮询不足时提供实时推送：

- session 状态变化
- turn 状态变化
- artifact metadata 更新
- approval notification（Post-MVP）

---

## 3. 与 MVP 的关系

MVP 阶段：

- 不实现 Web UI
- 不实现 WebSocket
- 不实现 SSE
- 不实现浏览器通知
- 不实现 approval inbox
- 不实现终端实时流

MVP 只要求 External API 足够完整，使未来 Web UI 可以基于同一 HTTP API 构建。

---

## 4. Web UI 依赖的后端能力

未来 Web UI 应优先复用 External API：

- `GET /external/v1/sessions`
- `GET /external/v1/sessions/{session_id}`
- `POST /external/v1/sessions/{session_id}/turns`
- `GET /external/v1/sessions/{session_id}/turns`
- `GET /external/v1/sessions/{session_id}/events`
- `GET /external/v1/sessions/{session_id}/artifacts`
- `GET /external/v1/artifacts/{artifact_id}/content`
- `POST /external/v1/sessions/{session_id}/interrupt`
- `POST /external/v1/sessions/{session_id}/restart`
- `DELETE /external/v1/sessions/{session_id}`

因此，MVP External API 的完整性比 Web UI 本身更重要。

---

## 5. WebSocket 后续设计原则

如果后续加入 WebSocket，应遵循：

- WebSocket 只作为状态推送优化，不替代 External API 写操作
- 写操作仍通过 HTTP 完成，便于审计和幂等
- WebSocket 消息应从 Control Plane 领域事件或投影变化派生
- 必须设计重连、补帧、去重和服务端重启语义
- WebSocket 不应暴露 runtime 或 client-specific 细节

具体协议、seq、buffer、断线补偿属于后续设计。

---

## 6. Approval UI（Post-MVP）

Approval / human-in-the-loop 不属于 MVP。未来若加入，应先完成领域设计：

- approval 是观察记录还是强控制
- 哪些 client 支持 approval enforcement
- approve / deny 如何传回 client
- deny 后 turn/session 状态如何变化
- approval 事件如何进入 event store

在这些问题解决前，Web UI 不应假设 approval 一定可用。

---

## 7. 终端预览（Post-MVP）

终端预览、terminal snapshot、实时输出流均为后续能力。

如果未来实现，应以 artifact 或 runtime diagnostic 的形式暴露，不应成为 session / turn 主状态来源。

---

## 8. 最终结论

Web UI 和 WebSocket 是 Control Plane 的后续产品层，不属于 MVP。MVP 应先完成稳定、可编排、HTTP-only 的后端 Control Plane；未来 Web UI 应建立在该 External API 之上，而不是反向驱动核心领域设计。
