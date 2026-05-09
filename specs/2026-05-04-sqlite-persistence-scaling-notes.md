# SQLite Persistence Scaling Notes

本文是性能优化 backlog，记录当前持久层在支持 20+ agent 并行运行时可能遇到的瓶颈，以及未来可以按优先级实施的 SQLite 优化方向。

当前暂不实施这些优化，仅作为性能加固依据。

## 背景

当前系统使用：

- SQLite 作为 domain event / session / turn / task / artifact 等结构化状态存储。
- 文件系统存储 runtime 目录、adapter event outbox、current turn context、artifact 原始内容等。
- 多个 agent session 会并行运行，并持续产生 session / turn / adapter 事件。

agent 数量超过 20 个时，主要持久层风险来自并发写入和写入放大，而不是 SQLite 单机容量本身。

## 当前主要瓶颈

### 1. SQLite 单写者模型

SQLite 同一时间只能有一个 writer。当前多个 async task 可以同时通过 `SqlitePool` 发起写入，最终会在 SQLite 写锁上竞争。

高并发写入时可能出现：

- 写入排队延迟升高。
- API p95 / p99 延迟升高。
- `database is locked` 错误。
- reader 与 writer 互相影响。

### 2. 每个 event 一个事务

当前 event ingest 路径大致是：

```text
begin transaction
insert events
update session projection
update turn projection
commit
```

如果 agent 高频输出事件，每条事件都独立事务会增加：

- 事务提交成本。
- fsync / WAL 写入压力。
- SQLite writer lock 获取次数。

### 3. event ingest 存在读写放大

每次 ingest event 时会读取已有 projection，并计算 session state version。随着单个 session 的 turns / events 增多，单次写入成本会逐渐升高。

风险表现：

- 长 session 越跑越慢。
- 高频事件下写入队列积压。
- 单个活跃 session 影响其他 session 的写入延迟。

### 4. adapter-events.jsonl 全量重放

当前 pi adapter event outbox 观察逻辑会读取整个 `adapter-events.jsonl`，并从头解析每一行。

随着日志文件增长，每次 observe 成本都会上升，并且会产生大量重复 ingest / duplicate check。

### 5. artifact discovery 全量扫描

artifact discovery 会递归扫描 workspace，并对文件做 metadata / preview / upsert。

多个 session 同时 discovery 大 workspace 时，会造成：

- 文件系统 I/O 压力。
- 大量 SQLite 小写入。
- API 请求阻塞时间变长。

### 6. 查询侧 N+1 与轮询压力

部分查询会先查列表，再逐条 enrich。SSE event stream 当前也会周期性查询 SQLite。

当 session / turn / event 数量增长后，查询侧会进一步放大 SQLite 压力。

## 后续优化方向

### 优先级 1：SQLite 基础加固

在 SQLite 连接初始化中显式配置：

```text
journal_mode = WAL
synchronous = NORMAL
busy_timeout = 5s 左右
```

并显式限制连接池大小，例如：

```text
max_connections = 4
```

预期收益：

- 减少读写互相阻塞。
- 降低 `database is locked` 发生概率。
- 让锁竞争从直接失败变成有限等待。
- 提升读写混合场景稳定性。

注意事项：

- WAL 会产生 `.db-wal` 和 `.db-shm` 文件。
- 需要关注 WAL 文件增长和 checkpoint。
- `busy_timeout` 会降低错误率，但高压时会增加请求等待时间。
- 这不是并发写银弹，SQLite 仍然只有一个 writer。

### 优先级 2：收口 event 写路径

不要让所有业务代码直接并发写 event store。建议引入 `EventStore` 作为持久层边界：

```text
SessionCommandService
TurnCommandService
RuntimeObservationService
PiAdapterEventOutboxService
        ↓
EventStore
        ↓
SQLite
```

对外保留简单接口：

```text
ingest(event)
list_events(session_id)
list_stream_items_after(...)
```

内部实现可以先继续使用 SQLx，后续再切换为写入队列或 batch，不影响业务层。

### 优先级 3：event 单写队列

在 `EventStore` 内部使用 bounded channel 和单独 writer task：

```text
业务 service
  ↓
EventStore::ingest(event)
  ↓
bounded mpsc channel
  ↓
single writer task
  ↓
SQLite transaction
```

预期收益：

- 避免多个 async task 同时抢 SQLite 写锁。
- 提供 backpressure。
- 保持 event 写入顺序更可控。
- 为后续 batch transaction 做准备。

队列建议：

- 使用 bounded channel，避免数据库跟不上时无限占用内存。
- 队列满时可以选择等待，或返回明确的繁忙错误。
- 初始可只队列化 event ingest，不必马上覆盖 artifact / task 等低频写入。

### 优先级 4：batch event transaction

在 writer task 中合并短时间内的多个 event，例如：

```text
最多 50 条 event
或最多等待 20-50ms
然后一次 transaction 写入
```

预期收益：

- 减少事务数量。
- 减少 commit / fsync 成本。
- 提高高频 event 写入吞吐。

需要注意：

- 当前 event ingest 会返回 `EventIngestResult`，batch 实现需要为 batch 内每个 event 分别返回结果。
- 同一 session 内事件顺序需要保持可解释。
- 错误处理需要明确：单条失败是否影响整个 batch。

### 优先级 5：减少 event ingest 写入放大

后续可以优化：

- 避免每次 event ingest 都加载 session 下所有 turn projection。
- 避免每次通过 `COUNT(*)` 计算 state version。
- 将 session / turn projection 更新改成更定向的读写。
- 对高频 `turn.output` 做合并或降采样策略。

目标是让单条 event 写入成本接近 O(1)，避免随着 event 数据增长而变慢。

### 优先级 6：adapter outbox 增量消费

为 `adapter-events.jsonl` 增加消费 cursor，例如记录：

```text
session_id
adapter_event_log_path
byte_offset
last_processed_at
```

每次 observe 时只读取新增内容，而不是全量读取和全量重放。

预期收益：

- 避免 outbox 文件越大 observe 越慢。
- 减少重复 JSON parse。
- 减少 duplicate ingest 查询。

### 优先级 7：artifact discovery 限流和批量写

artifact discovery 后续可以优化为：

- 对 discovery 做并发限制。
- preview 只读取文件前 N bytes，不完整读取整个文件。
- artifact upsert 使用批量事务。
- 跳过明显无关目录，例如 `.git`、`node_modules`、构建输出目录等。
- 根据 mtime / size / fingerprint 做增量发现。

### 优先级 8：查询侧优化

后续可以优化：

- 减少 list sessions / list turns 的 N+1 enrich 查询。
- 为 event stream 的 cursor 查询设计更匹配的索引或显式 offset 字段。
- 降低空闲 SSE 轮询频率，或引入事件通知机制。
- 对 events 做分页，避免一次性返回过大结果集。

## 建议触发条件

暂时不做优化是合理的，但出现以下信号时应优先处理本文中的优化项：

- 20+ agent 并行时出现 `database is locked`。
- event ingest p95 / p99 延迟明显升高。
- API 请求经常卡在写入或事件查询。
- `adapter-events.jsonl` 文件增长后 observe 明显变慢。
- `.db-wal` 文件持续增长且无法回收。
- artifact discovery 影响正常 session / turn 操作。
- 单个长 session 事件量增长后变慢。

## 推荐实施顺序

开始优化时，建议按以下顺序推进：

1. 开启 WAL、busy_timeout、synchronous=NORMAL，并限制 SQLite pool size。
2. 抽象 `EventStore`，把 event ingest 作为明确持久层边界。
3. 在 `EventStore` 内实现单写队列。
4. 在 writer task 内实现 batch event transaction。
5. 为 adapter event outbox 增加 byte offset cursor。
6. 优化 event ingest 的 projection reload 和 state version 计算。
7. 优化 artifact discovery 的扫描、preview 和批量 upsert。
8. 优化 event stream 和列表查询索引/分页。

## 当前决策

当前阶段暂不实施上述优化。

原因：

- 现阶段更需要保持实现简单。
- 20+ agent 并行尚未成为已验证瓶颈。
- SQLite 基础能力足以支持低频事件写入的本地单机使用。
- 后续可以通过 `EventStore` 边界逐步演进，不需要现在一次性重构所有持久层代码。
