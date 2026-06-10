# Agent Bindings 与 Transcript 按需解析设计

日期：2026-06-09

本文记录 pilotfy WebUI 展示 agent 中间过程所需的数据链路设计。目标是让 WebUI 能展示 agent client TUI 中可见的完整过程，同时避免把 agent client 的原始 transcript 拆分后持久化到 SQLite。

## 核心结论

采用：

```text
agent_bindings 只保存 pilotfy session 与真实 agent runtime 的绑定关系
transcript 内容按请求实时解析
```

总体链路：

```text
WebUI request
  -> External API
  -> agent_bindings 查到 client_type + launch_cwd + client_session_key
  -> client runtime resolver 计算原始文件位置
  -> 后端读取文件或文件片段
  -> client-specific parser 解析
  -> 返回 normalized timeline/detail content
```

第一阶段不把 JSONL/transcript 拆成持久化表。拆分只发生在 parser 内存过程里：

```text
raw JSONL / transcript bytes
  -> parser 内部拆分 message / tool call / tool result / bash output
  -> response DTO
```

## agent_bindings schema


`agent_bindings` 不存物理 path，只存 pilotfy session 与真实 agent runtime 的稳定绑定 identity。

```sql
CREATE TABLE agent_bindings (
  id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  client_type TEXT NOT NULL,
  launch_cwd TEXT NOT NULL,
  client_session_key TEXT NOT NULL,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  FOREIGN KEY(session_id) REFERENCES sessions(session_id),
  UNIQUE(session_id, client_type, client_session_key)
);
```

建议索引：

```sql
CREATE INDEX idx_agent_bindings_session ON agent_bindings(session_id, id);
CREATE INDEX idx_agent_bindings_identity ON agent_bindings(client_type, launch_cwd, client_session_key);
```

字段说明：

- `id`：pilotfy 内部 agent binding id；在 SQL 中通过表名上下文表达语义，即 `agent_bindings.id`。
- `session_id`：pilotfy session id。
- `client_type`：agent client 类型，例如 `pi`。
- `launch_cwd`：agent runtime 启动时使用的真实 canonical cwd。
- `client_session_key`：agent client 自己的 session identity。
- `metadata`：诊断信息；不存权威 physical path。

Agent bindings 和 sessions 的关系

```text
pilotfy_session_id
  -> agent_bindings row
  -> client_type + launch_cwd + client_session_key
  -> resolver
  -> raw file
  -> parser
```

## launch_cwd snapshot

`launch_cwd` 是真实 agent runtime 的启动 cwd snapshot，也是后续定位 client transcript/output 的 resolver 输入；

语义：

```text
agent_bindings.launch_cwd = 该 pilotfy session 绑定的真实 agent runtime 启动时的 canonical cwd
```

不变量：

1. agent runtime binding 注册时写入。
2. 写入后不应修改。
3. resolver 使用该字段，而不是当前 workspace path。
4. `sessions.workspace_id` 继续表示 pilotfy 业务 workspace；`agent_bindings.launch_cwd` 表示 agent client raw output 定位所需 cwd。

多数情况下：

```text
agent_bindings.launch_cwd == workspaces.canonical_path
```

但二者语义不同。

## Resolver 抽象

Resolver 负责根据 `agent_bindings` 中的 runtime binding identity 定位 agent client 原始输出。

```rust
pub struct AgentBindingResolveRequest {
    pub id: String,
    pub session_id: String,
    pub client_type: String,
    pub launch_cwd: PathBuf,
    pub client_session_key: String,
}

pub struct ResolvedAgentBinding {
    pub id: String,
    pub client_type: String,
    pub format: String,
    // path 只在后端内存中使用，不存 DB；第一阶段可作为明文 source_id 的组成部分返回给 WebUI
    pub path: PathBuf,
    pub fingerprint: Option<String>,
}

pub trait AgentBindingResolver {
    fn client_type(&self) -> &'static str;
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding>;
}
```

每个 client 实现自己的 resolver：

```text
pi resolver:
  launch_cwd + client_session_key
  -> pi session directory/path rule
  -> session JSONL path

future claude resolver:
  launch_cwd + client_session_key
  -> claude transcript path rule
```

物理路径规则属于 agent client adapter/parser 边界，不泄漏给 WebUI。

## Parser 抽象

Parser 负责把 client-specific 原始格式转换成统一 response DTO。

```rust
pub struct TimelinePageRequest {
    pub session_id: String,
    pub source: ResolvedAgentBinding,
    pub cursor: Option<String>,
    pub limit: usize,
}

pub struct TimelineItemDetailRequest {
    pub session_id: String,
    pub source: ResolvedAgentBinding,
    pub content_ref: TimelineContentRef,
}

pub trait RawTranscriptParser {
    fn client_type(&self) -> &'static str;
    fn format(&self) -> &'static str;
    fn timeline_page(&self, request: TimelinePageRequest) -> Result<TimelinePage>;
    fn timeline_item_detail(&self, request: TimelineItemDetailRequest) -> Result<TimelineItemDetailPage>;
}
```

Parser 可以选择：

- 每次从头扫描 JSONL 到目标范围。
- 根据 cursor 从某个 byte offset 继续读。
- 根据 client 文件格式读取部分文件。
- 根据 `content_ref` 精确读取单个 item 的完整内容。

这些优化都不要求持久化 transcript 拆分结果。

## agent binding 注册时机

agent binding 需要 `session_id`、`client_type`、`launch_cwd`、`client_session_key`。

推荐注册边界是 agent client 的 `session.ready` 事件处理过程。

原因：

1. session 创建/启动时，后端通常已经知道 `session_id`、`client_type`、runtime workspace/cwd。
2. agent client ready 时，client/plugin 才稳定知道 `client_session_key`。
3. `session.ready` 是 pilotfy session 与真实 agent runtime identity 首次同时可用的边界。

因此推荐流程：

```text
agent client reports session.ready
  -> backend 校验 session 存在
  -> ingest/project session.ready event
  -> 同步 upsert agent_bindings
  -> 返回 accepted
```

如果实现上允许，`session.ready` ingest/projection 与 `agent_bindings` upsert 应尽量在同一事务内完成，避免 dashboard SSE 先暴露 ready、但 timeline API 尚无法 resolve binding 的竞态。

`agent_bindings` upsert 必须幂等。重复的 `session.ready` 或 Internal Event API retry 不应创建重复 binding；同一 `session_id + client_type + client_session_key` 应稳定落到同一 binding 语义。

对于 pi，理想情况是 pi extension 在 ready 阶段上报 client session key；后端结合 runtime launch context 中的 canonical cwd 注册 `agent_bindings`。如果 ready payload 与后端 launch context 都提供 cwd，第一阶段以后端 launch context 为准，ready payload 中的 cwd 仅作为诊断 metadata。

## External API

第一阶段建议提供：

```text
GET /external/v1/sessions/{session_id}/timeline?cursor=&limit=
GET /external/v1/sessions/{session_id}/timeline/detail?ref=
```

第一阶段一个 pilotfy session 默认绑定一个 primary agent runtime；必要时后续可以加入 `binding_id` query 参数选择特定 binding。

### Timeline response

```json
{
  "session_id": "sess_xxx",
  "binding_id": "bind_xxx",
  "items": [
    {
      "item_id": "pi:entry:abc123:block:0",
      "kind": "assistant_message",
      "role": "assistant",
      "title": null,
      "status": null,
      "occurred_at": "2026-06-09T00:00:00Z",
      "content_preview": "我先看看代码",
      "content_ref": "pi:entry:abc123:block:0"
    },
    {
      "item_id": "pi:entry:abc123:block:1",
      "kind": "tool_call",
      "role": "tool",
      "title": "read",
      "status": "started",
      "occurred_at": "2026-06-09T00:00:01Z",
      "content_preview": "read src/main.rs",
      "content_ref": "pi:entry:abc123:block:1"
    }
  ],
  "next_cursor": "cursor_v1_opaque_token",
  "tail_cursor": "cursor_v1_opaque_token",
  "has_more": true,
  "is_tail": false,
  "source_id": "pi:/home/user/.local/share/pi/sessions/session.jsonl"
}
```

字段语义：

- `next_cursor`：继续读取下一段 timeline items 的 cursor；当 `has_more = true` 时前端可用它继续请求。
- `tail_cursor`：本次 response 已读取到的 source 位置；当前端已经持有 timeline state 时，用它作为后续 `session.message_updated` 触发增量读取的起点。
- `has_more`：当前 cursor 之后仍有未返回 items。
- `is_tail`：本次 response 是否已经追到 raw source 当前尾部。
- `source_id`：raw source identity。第一阶段使用明文 `client_type + ':' + resolved_path`，用于 WebUI 将 timeline items 与 cursor 作为同一个状态单元管理。mtime/size/inode 不作为 cursor 失效条件。

`content_ref` 是 opaque token。WebUI 不解析其中结构，只把它传回 timeline detail API。

### Timeline 与 detail 的读取边界

`timeline` 与 `timeline/detail` 都可能读取同一个 agent raw transcript source，但后端逻辑不同：

```text
timeline:
  cursor/offset -> 顺序向后扫描 raw transcript -> 返回 normalized items + preview + content_ref

timeline/detail:
  content_ref(start/end/block info) -> bounded read -> 返回单个 item 的完整内容
```

设计原则：

- `timeline` 是唯一负责发现 transcript message/block 的 API。
- `timeline/detail` 不做 transcript discovery，也不提供从头全量提取 raw transcript 的能力。
- `timeline` 的 cursor/offset 表示 raw transcript stream 中的扫描位置。
- `timeline/detail` 第一阶段不提供 detail 内部分页；它返回 `content_ref` 指向的单个 item 完整内容，避免 offset/limit 截断 JSON 或结构化 block 内部。
- `content_ref` 可包含后端 opaque 的定位信息，例如 binding/source scope、source_id、entry start/end byte offset、block index。WebUI 不解析这些信息。

### Cursor-based timeline read model

Timeline API 只有一种读取语义：从某个 cursor 之后读取下一段 normalized timeline items。

- `cursor` absent/null 表示从 raw source 起点读取。
- `cursor = next_cursor` 表示继续读取当前未读完的后续 page。
- `cursor = tail_cursor` 表示从前端已经读到的位置之后读取新增 items。

Cursor 对 WebUI 是 opaque token：WebUI 不解析、不构造、不假设其内部格式，只保存 API 返回的 cursor 并原样传回。

第一阶段采用 parser-specific opaque cursor。即 cursor 的内部结构由具体 parser 决定，但对 External API consumer 不透明。pi JSONL parser 内部可使用 byte offset，并把必要上下文编码进 cursor，例如：

```json
{
  "v": 1,
  "kind": "timeline",
  "client_type": "pi",
  "format": "pi-jsonl",
  "binding_id": "bind_xxx",
  "source_id": "pi:/home/user/.local/share/pi/sessions/session.jsonl",
  "offset": 123456
}
```

实际 wire format 可以是 base64url JSON 或其它后端可解析的 token。后续其他 parser 可以使用 entry id、sequence id、timestamp 或其它定位信息，不影响 WebUI。

请求携带 cursor 时，后端不做 mtime/size/inode 等提前强校验。后端 decode cursor 后 resolve 当前 path，并从 cursor offset 实际 seek/read/parse；如果读取过程中发现文件不存在、offset 无法读取、offset 不在合法边界、JSONL parse 失败或 source_id 明显不匹配，则返回 cursor invalid / source unavailable，前端清空 TimelineState 并从 `cursor = null` 重建。

因此 initial rebuild 与 live follow 不是两套 API 模式：

```text
initial rebuild = cursor absent/null 的增量读取，因为起点是 0，所以表现为全量重建
live follow     = cursor = 前端保存的 tail_cursor 的增量读取
```

前端没有本地 timeline state 时，使用 `cursor = null` 请求并 replace 本地 state；前端已有 timeline state 时，收到刷新 hint 后使用本地 `tail_cursor` 请求并 append 返回 items。

### Frontend timeline state ownership

第一阶段 cursor 由 WebUI 和 timeline items 作为同一个状态单元维护。后端不保存 parser checkpoint/cache。

推荐前端状态：

```ts
type TimelineState = {
  sessionId: string
  bindingId: string | null
  items: TimelineItem[]
  nextCursor: string | null
  tailCursor: string | null
  sourceId: string | null
  hasMore: boolean
  isTail: boolean
  loading: boolean
  refreshing: boolean
  error: string | null
}
```

`items + nextCursor + tailCursor + sourceId` 必须一起生效、一起失效，避免重复消息、漏消息或 cursor 与列表不匹配。

失效条件包括：

- `session_id` 变化。
- `binding_id` / `source_id` 变化。
- API 返回 cursor invalid / source unavailable / content_ref_invalid。
- 用户手动 refresh。
- append 时发现无法 reconcile 的断层或重复。

失效后 WebUI 清空 items 和 cursor，从 `cursor = null` 重新读取。

### Timeline item detail response

```json
{
  "binding_id": "bind_xxx",
  "content_ref": "pi:entry:abc123:block:1",
  "content_type": "application/json",
  "text": "{...}",
  "size_bytes": 1234567
}
```

第一阶段 detail API 全量返回单个 item 内容，不做 offset/limit 截断或分页。大内容保护依赖服务端统一响应大小限制或后续版本引入结构感知分页；第一阶段不在 detail API 中实现 byte-range 分页。

## pi parser 第一版映射

pi session JSONL 中的结构映射为统一 timeline item：

| pi entry/message | timeline kind |
| --- | --- |
| `message.role = user` | `user_message` |
| `message.role = assistant` text block | `assistant_message` |
| `message.role = assistant` thinking block | `assistant_thinking` |
| `message.role = assistant` toolCall block | `tool_call` |
| `message.role = toolResult` | `tool_result` |
| `message.role = bashExecution` | `user_bash` |
| `message.role = custom` | `custom_message` |
| `message.role = branchSummary` | `branch_summary` |
| `message.role = compactionSummary` | `compaction_summary` |
| `type = model_change` | `model_change` |

pi timeline detail 读取规则：

- 普通 entry/block：读取 pi session JSONL 中对应 entry/block。
- `bashExecution.fullOutputPath`：优先读取该 full output 文件，第一阶段全量返回。
- 没有 `fullOutputPath` 时，读取 JSONL entry 中的 `output` 字段。

## 与 events / turns 的关系

`events` 仍然保存 pilotfy session/turn/domain facts。

Raw transcript timeline 是按需 read model，不进入 event store。它的权威来源是 agent client raw structured output 文件。

第一阶段 raw transcript timeline 以 `session_id` 为范围展示，timeline item 不做 item-level `turn_id` 归属推导；`turn_id` 仅作为 optional annotation。也就是说，WebUI 展示 agent 中间过程时可以直接使用 raw timeline 中的 role/block 结构分节，不要求每条 item 都绑定到某个 pilotfy turn。

但这不弱化 Turn 在 pilotfy domain model 中的地位：Turn 仍然是执行生命周期和控制对象。Turn 的状态、完成、失败、interrupt/cancel/retry 等控制语义继续由 event store/projection 负责，不从 raw transcript 推断。

后续如果某个 client 的 raw transcript 能稳定提供 pilotfy turn identity，parser 可以在 timeline item 上填充 `turn_id`；否则保持 `null`，不要通过时间窗口或 current-turn context 伪造归属。

`agent_bindings` 只是 session-to-agent-runtime binding 表，不是 transcript 拆分表，也不表达 raw file 的当前可用状态。

### Message update event

为了让 WebUI 知道何时刷新 timeline，新增 domain event：

```text
session.message_updated
```

`session.message_updated` 写入 `events` 表，并通过现有 dashboard/session SSE 传播。它表示 agent client transcript/timeline 中有一条可展示 message/block 新增或更新。

示例 payload 保持轻量：

```json
{
  "binding_id": "bind_xxx",
  "reason": "append"
}
```

`reason` 可取值：

- `append`：新增了可展示 message/block。
- `update`：已有 message/block 内容更新，例如 streaming output 继续增长。
- `final`：终态刷新，通常在 turn completed / failed / interrupted 时强制上报。

语义：

- `session.message_updated` 是 invalidation/refresh hint，不承载 transcript 内容。
- event payload 只携带 `binding_id` 和轻量 `reason`；不携带 `source_id`、`content_ref`、message 内容或 tail cursor，这些信息由 timeline API response 负责返回。
- event 不取代 timeline API；WebUI 收到 event 后仍需使用本地 `tailCursor` 调用 timeline API 获取 normalized items。
- 该 event 不改变 session/turn/task projection；projection 必须忽略它。
- 前端只对当前打开的 session 触发增量读取；如果 payload 中的 `binding_id` 与本地 timeline state 不匹配，可以清空 state 后从 `cursor = null` 重建。

推荐触发时机：

- agent runtime 产生完整 message/block 时触发。
- tool call / tool result 出现时触发。
- 大量 streaming output 需要 debounce/coalesce，避免高频写入 event store。
- turn completed / failed / interrupted 时强制触发一次 final update。

由于该事件会较频繁落库，payload 必须保持小且稳定。第一阶段可接受事件粒度为“有一个可展示的小节发生变化就触发一次”，但 runtime/plugin 应做轻量合并，例如 busy 期间最多每 250ms~500ms 上报一次，终态事件不合并丢失。

## 实施顺序

1. 新增 `agent_bindings` migration。
2. session/runtime/pi extension 链路注册 `agent_bindings`。
3. 实现 agent binding resolver trait。
4. 实现 pi resolver。
5. 实现 raw transcript parser trait。
6. 实现 pi JSONL parser。
7. 新增 timeline API。
8. 新增 timeline detail API。
9. 新增 `session.message_updated` event 类型与 runtime/plugin 上报链路。
10. WebUI 改为消费 timeline/detail API，并维护 `TimelineState`。

## 非目标

第一阶段不处理：

- 把 agent transcript 拆分持久化到 SQLite。
- 增量 import job。
- 后端 parser checkpoint/cache 或 parser 结果缓存。
- 持久化 WebUI timeline state；第一阶段 timeline state 只作为前端页面状态。
- 在 `session.message_updated` event 中承载 transcript 内容、source path、content refs 或 cursor。
- agent client raw file 路径规则变化后的自动迁移。
- WebUI 直接读取本地文件。
- Claude Code / Codex parser 的完整实现。

如果 resolver 找不到 raw file，API 应返回明确的 unsupported/unavailable/degraded 结果，而不是猜测路径或返回伪造数据。

## 待确认问题

1. pi `client_session_key` 的稳定来源和写入时机。
