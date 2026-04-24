# Control Plane Client Filesystem / Artifact Adapter 设计

## 1. 文档目标

本文定义 MVP 阶段 Artifact Adapter 的设计边界。

Artifact Adapter 负责把 agent client 产生的文件、日志、结果、transcript 等内容以统一索引暴露给 External API。它是内容与索引系统，不默认作为 session / turn 主状态事实源。

本文保持在设计层面，不规定具体文件监听机制、SQLite schema、索引字段或 client 特定路径。

---

## 2. 设计原则

### 2.1 Control Plane 是对外查询入口

External API 调用方通过 Control Plane 查询 artifact metadata 和内容，不直接依赖 client 的目录结构。

### 2.2 原生文件是内容来源

大内容保留在文件系统或 artifact storage 中。Control Plane 保存：

- artifact identity
- source reference
- metadata
- content location
- preview / summary
- cursor or indexing state

### 2.3 Artifact 不默认改变领域状态

Artifact 发现、索引或更新通常不改变 session / turn 状态。只有当 adapter 明确产生领域事件时，artifact 相关信息才影响领域投影。

### 2.4 Client 差异由 adapter 收敛

不同 client 的日志、transcript、输出文件路径不同。Artifact Adapter 应把这些差异收敛为统一 ArtifactView。

---

## 3. 职责边界

Artifact Adapter 负责：

- 注册 artifact source
- 发现 artifact
- 更新 artifact metadata
- 维护索引进度
- 提供 artifact preview
- 支持 External API 读取内容
- 对大文件提供 range / chunk 读取语义

Artifact Adapter 不负责：

- 判断 turn 是否完成
- 解析 agent 内部语义作为主状态
- 执行 runtime 控制
- 处理 Approval
- 替代 Internal Event API
- 存储完整大日志到领域事件 payload

---

## 4. 核心对象

### 4.1 ArtifactSource

表示某个 session 下的内容来源，例如：

- workspace output directory
- client log directory
- transcript file
- fallback event queue
- runtime diagnostic file

### 4.2 ClientArtifact

表示一个可查询和读取的内容对象，例如：

- log file
- transcript
- generated patch
- test report
- terminal snapshot
- model output file

### 4.3 ArtifactCursor

表示 Adapter 对某个 source 的索引进度。Cursor 是辅助状态，不参与领域事件重放承诺。

---

## 5. External API 能力

MVP 需要支持：

```http
GET /external/v1/sessions/{session_id}/artifacts
GET /external/v1/artifacts/{artifact_id}
GET /external/v1/artifacts/{artifact_id}/content
```

内容读取应支持大文件友好的语义，例如 range 或分页读取。具体 header、chunk 策略和缓存策略属于 API 细化阶段。

---

## 6. 与事件模型的关系

Artifact metadata 可以被事件 payload 引用，例如：

```json
{
  "artifact_id": "art_...",
  "kind": "log",
  "preview": "...",
  "size_bytes": 12345
}
```

但完整内容不应进入 event payload。

如果某个 artifact 的出现代表领域事实，例如“turn result file generated”，adapter 可以同时发布领域事件。但默认情况下，artifact 索引只是内容可见性，不改变 session / turn 状态。

---

## 7. 安全边界

设计上需要保证：

- External API 只能读取已注册 artifact source 下的内容
- 不暴露任意文件路径读取能力
- metadata 和 preview 应避免泄露 token
- 大内容读取应可限制大小和范围

具体路径规范化、权限位、脱敏算法属于实现阶段。

---

## 8. MVP 范围

### 8.1 MVP 必做

- Artifact source 抽象
- Artifact metadata 查询
- Artifact content 读取
- Artifact 与 session / turn 的关联
- Artifact 不作为主状态事实源的边界说明

### 8.2 Post-MVP

- WebSocket artifact 推送
- client-specific artifact parser
- 复杂 log rotation 策略
- 全文搜索
- artifact retention / compaction
- artifact event replay

---

## 9. 最终结论

Artifact Adapter 是 Control Plane 的内容索引层。它让 External API 能统一读取 agent 产生的文件和日志，但不承担 session / turn 主状态维护职责。MVP 只定义统一查询和内容读取边界，具体 client 文件布局后续再适配。
