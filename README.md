# llmparty

`llmparty` 是一个面向 Coding Agent 的控制台和控制平面。你可以用它启动并管理 agent session，向 agent 提交任务，查看执行过程、结果和产物，并在需要时中断、重启或终止 session。

它适合这些场景：

- 用浏览器统一操作本地或远程的 coding agent
- 用 HTTP API 把 agent 接入自己的脚本、自动化流程或上层 Orchestrator
- 用同一套 session / turn / event / artifact 模型管理不同 agent client
- 通过 `tmux` 保持 agent runtime 长时间运行，而不是每次任务都临时启动一个进程

当前已支持 `generic` 测试 client 和 `pi` client。

## 功能概览

- 创建、查看和管理 agent session
- 向 session 提交多轮任务 / prompt
- 实时查看事件流和 turn 输出
- 查看 session 产生的文件 artifact
- 中断、重启、终止 session
- Web Dashboard 浏览器界面
- HTTP External API，方便脚本和系统集成
- pi client 的本地运行和结果回传

## 准备环境

需要安装：

- Rust / Cargo
- tmux
- pnpm（用于 Web Dashboard）
- pi CLI（如果要使用 `client_type = "pi"`）

## 快速开始

### 1. 配置环境变量

```bash
cp .env.example .env
```

默认配置会监听 `127.0.0.1:8080`，并使用 `dev-token` 作为访问 Dashboard 和 External API 的 token。

### 2. 安装并构建 Dashboard

```bash
pnpm --dir apps/web install
pnpm --dir apps/web build
```

### 3. 启动 llmparty

```bash
cargo run
```

服务启动后可以检查健康状态：

```bash
curl http://127.0.0.1:8080/healthz
```

返回：

```json
{"status":"ok"}
```

### 4. 打开 Dashboard

访问：

```text
http://127.0.0.1:8080/dashboard
```

在页面中输入 token，例如：

```text
dev-token
```

之后即可创建 session、提交任务、查看事件和结果。

## 使用 Dashboard

Dashboard 是最推荐的本地使用方式。

常用流程：

1. 打开 `/dashboard`
2. 输入 External API token
3. 创建一个 session
4. 选择 client 类型：
   - `generic`：用于验证流程和 API
   - `pi`：使用真实 pi client
5. 填写 workspace 路径
6. 提交任务
7. 在事件流和输出区域查看 agent 回答
8. 如有需要，查看 artifacts、重启或终止 session

如果一个 session 正在执行 turn，Dashboard 会暂时禁用新的提交，直到当前 turn 完成或失败。

## 使用 pi client

使用 pi 前，请确认本机可以直接运行：

```bash
pi
```

启动 llmparty 时建议显式配置内部事件上报地址：

```bash
LLMPARTY_EXTERNAL_API_TOKEN=dev-token \
LLMPARTY_INTERNAL_EVENT_URL=http://127.0.0.1:8080/internal/v1/events \
cargo run
```

然后在 Dashboard 中创建 `client_type = "pi"` 的 session。

如果需要让 llmparty 使用指定的 pi 命令或本地扩展，可以设置：

```bash
LLMPARTY_PI_TUI_COMMAND='pi -e /absolute/path/to/llmparty/clients/pi'
```

pi session 运行时，llmparty 会在全局 runtime 目录下维护自身状态文件：

```text
~/.local/share/llmparty/runtimes/<session_id>/current-turn.json
~/.local/share/llmparty/runtimes/<session_id>/pi-hook.log
```

workspace 仅作为 runtime cwd，不再创建项目内 `.llmparty/`。如果 Dashboard 没有收到 pi 的输出或完成事件，可以先查看对应 runtime 目录中的 `pi-hook.log`。

## 使用 HTTP API

所有对外 API 都在 `/external/v1/*` 下，需要 Bearer token。

下面示例假设服务运行在 `127.0.0.1:8080`，token 为 `dev-token`。

### 创建 session

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: demo-session-1' \
  -d '{
    "client_type":"generic",
    "workspace":"/tmp/llmparty-demo",
    "initial_task":{"input":"请介绍一下当前项目"}
  }'
```

### 查看 session 列表

```bash
curl http://127.0.0.1:8080/external/v1/sessions \
  -H 'Authorization: Bearer dev-token'
```

### 提交下一轮任务

把 `sess_example` 替换为实际返回的 session id。

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/turns \
  -H 'Authorization: Bearer dev-token' \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: demo-turn-1' \
  -d '{"input":"继续完成下一步"}'
```

### 查看事件

```bash
curl http://127.0.0.1:8080/external/v1/sessions/sess_example/events \
  -H 'Authorization: Bearer dev-token'
```

### 实时订阅事件流

```bash
curl -N http://127.0.0.1:8080/external/v1/sessions/sess_example/events/stream \
  -H 'Authorization: Bearer dev-token'
```

### 查看 artifacts

```bash
curl http://127.0.0.1:8080/external/v1/sessions/sess_example/artifacts \
  -H 'Authorization: Bearer dev-token'
```

### 扫描 workspace artifacts

```bash
curl -X POST http://127.0.0.1:8080/external/v1/sessions/sess_example/artifacts/discover \
  -H 'Authorization: Bearer dev-token'
```

### 终止 session

```bash
curl -X DELETE http://127.0.0.1:8080/external/v1/sessions/sess_example \
  -H 'Authorization: Bearer dev-token' \
  -H 'Idempotency-Key: demo-terminate-1'
```

## 常用配置

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `LLMPARTY_BIND_ADDR` | `127.0.0.1:8080` | 服务监听地址 |
| `LLMPARTY_DATABASE_URL` | `sqlite://~/.local/share/llmparty/llmparty.db` | SQLite 数据库地址 |
| `LLMPARTY_EXTERNAL_API_TOKEN` | 未设置 | Dashboard 和 External API 的 Bearer token |
| `LLMPARTY_RUN_MIGRATIONS` | `true` | 启动时自动执行数据库迁移 |
| `LLMPARTY_INTERNAL_EVENT_URL` | 自动推导或手动设置 | agent / hook 回传事件的地址 |
| `LLMPARTY_PI_TUI_COMMAND` | `pi` | pi session 使用的启动命令 |

## 开发命令

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
pnpm --dir apps/web typecheck
pnpm --dir apps/web build
```

更多产品设计、API 契约和内部实现说明请查看 `spec/` 和 `docs/`。README 只保留对外使用所需的信息。
