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
  -> 返回 normalized timeline/content
```

第一阶段不把 JSONL/transcript 拆成持久化表。拆分只发生在 parser 内存过程里：

```text
raw JSONL / transcript bytes
  -> parser 内部拆分 message / tool call / tool result / bash output
  -> response DTO
```

## 为什么需要 agent_bindings 表

虽然 transcript 不做持久化拆分，但仍然需要一张 `agent_bindings` 表记录 pilotfy session 与真实 agent runtime/session 之间的稳定绑定关系。

不用把这些字段直接放到 `sessions` 的原因：

1. **职责更清楚**
   - `sessions`：pilotfy runtime/session 状态。
   - `agent_bindings`：pilotfy session 到真实 agent runtime 的绑定信息。

2. **避免污染 generic session**
   - 不同 client 的 runtime identity 不同。
   - generic `sessions` 不需要承载 pi/Claude/Codex 各自的 session key、启动 cwd 等定位参数。

3. **resolver 输入明确**
   - parser/resolver 不需要从 session metadata 里猜。
   - 直接读取 `agent_bindings` 的 runtime binding identity。

因此最终定位关系是：

```text
pilotfy_session_id
  -> agent_bindings row
  -> client_type + launch_cwd + client_session_key
  -> resolver
  -> raw file
  -> parser
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

## launch_cwd 放在 agent_bindings 中

`launch_cwd` 是真实 agent runtime 的启动 cwd，也是后续定位 client transcript/output 的 resolver 输入；它不是 pilotfy session 状态机的核心字段。因此第一阶段放在 `agent_bindings` 中，不新增 `sessions.launch_cwd`。

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

## 不存 path

`agent_bindings` 不存 computed file path。

原因：

- path 是 agent client 存储规则的结果，不是 pilotfy 的业务事实。
- 如果 agent client 的路径规则变化到自己都无法读取旧 session，这属于 agent client 层的大变更，不是 pilotfy 第一阶段要修复的问题。
- pilotfy 依赖稳定的 logical identity：`client_type + launch_cwd + client_session_key`。

Resolver 每次请求时根据上述 identity 计算实际文件位置。

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
    // path 只在后端内存中使用，不返回给 WebUI，不存 DB
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

pub struct RawContentRequest {
    pub session_id: String,
    pub source: ResolvedAgentBinding,
    pub item_ref: RawItemRef,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

pub trait RawTranscriptParser {
    fn client_type(&self) -> &'static str;
    fn format(&self) -> &'static str;
    fn timeline_page(&self, request: TimelinePageRequest) -> Result<TimelinePage>;
    fn content(&self, request: RawContentRequest) -> Result<RawContentPage>;
}
```

Parser 可以选择：

- 每次从头扫描 JSONL 到目标范围。
- 根据 cursor 从某个 byte offset 继续读。
- 根据 client 文件格式读取部分文件。
- 对大型 external output 文件做 offset/limit 读取。

这些优化都不要求持久化 transcript 拆分结果。

## agent binding 注册时机

agent binding 需要 `session_id`、`client_type`、`launch_cwd`、`client_session_key`。

推荐写入时机：

1. session 创建/启动时，后端知道 `session_id`、`client_type`、runtime workspace/cwd。
2. agent client ready/startup 时，client/plugin 知道 `client_session_key`。
3. 当两者都可用时注册或 upsert `agent_bindings`。

对于 pi，理想情况是 pi extension 在 `session_start` / ready 阶段上报 client session key；后端结合 launch cwd 注册 `agent_bindings`。

## External API

第一阶段建议提供：

```text
GET /external/v1/sessions/{session_id}/timeline?cursor=&limit=
GET /external/v1/sessions/{session_id}/raw-content?ref=&offset=&limit=
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
  "next_cursor": "byte:123456",
  "has_more": true
}
```

`content_ref` 是 opaque token。WebUI 不解析其中结构，只把它传回 content API。

### Raw content response

```json
{
  "binding_id": "bind_xxx",
  "content_ref": "pi:entry:abc123:block:1",
  "content_type": "application/json",
  "text": "{...}",
  "offset": 0,
  "returned_bytes": 65536,
  "total_bytes": 1234567,
  "next_offset": 65536
}
```

大内容不永久截断，通过分页读取。

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

pi content 读取规则：

- 普通 entry/block：读取 pi session JSONL 中对应 entry/block。
- `bashExecution.fullOutputPath`：优先读取该 full output 文件，支持 offset/limit。
- 没有 `fullOutputPath` 时，读取 JSONL entry 中的 `output` 字段。

## 与 events 的关系

`events` 仍然保存 pilotfy session/turn/domain facts。

Raw transcript timeline 是按需 read model，不进入 event store。它的权威来源是 agent client raw structured output 文件。

`agent_bindings` 只是 session-to-agent-runtime binding 表，不是 transcript 拆分表，也不表达 raw file 的当前可用状态。

## 实施顺序

1. 新增 `agent_bindings` migration。
2. session/runtime/pi extension 链路注册 `agent_bindings`。
3. 实现 agent binding resolver trait。
4. 实现 pi resolver。
5. 实现 raw transcript parser trait。
6. 实现 pi JSONL parser。
7. 新增 timeline API。
8. 新增 raw content API。
9. WebUI 改为消费 timeline/content API。

## 非目标

第一阶段不处理：

- 把 agent transcript 拆分持久化到 SQLite。
- 增量 import job。
- parser 结果缓存。
- agent client raw file 路径规则变化后的自动迁移。
- WebUI 直接读取本地文件。
- Claude Code / Codex parser 的完整实现。

如果 resolver 找不到 raw file，API 应返回明确的 unsupported/unavailable/degraded 结果，而不是猜测路径或返回伪造数据。

## 待确认问题

1. pi `client_session_key` 的稳定来源和写入时机。
2. timeline cursor 格式：byte offset、entry id，还是 parser-specific opaque cursor。
3. content API 默认 page size 与最大 page size。
4. turn_id 关联是否由 raw transcript、current-turn context、时间窗口或 pilotfy events 辅助推导。
