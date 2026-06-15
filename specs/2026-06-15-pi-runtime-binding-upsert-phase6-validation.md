# pi runtime binding upsert Phase 6 验证记录

日期：2026-06-15

## 已完成清理

- 将 runtime 层内部 `RuntimeStartResult.runtime_ref` 命名清理为 `runtime_handle`，避免与已移除的 `runtime_bindings.runtime_ref` schema/业务概念混淆。
- 将 in-process runtime metadata 从 `in_process.runtime_key` 改为 `in_process.runtime_handle`；runtime observation/control 保留对旧 `runtime_key` 的兼容读取，用于已有运行态/旧 metadata。
- 将 DAG patch helper 中非 runtime 语义的 `resolve_runtime_ref` 改名为 `resolve_temp_id_ref`。
- 将 pi extension 局部 `attached` 命名改为 `bindingResult`，避免继续表达旧 attached 分支语义。
- 更新 `README.md` 与 `clients/pi/README.md`，说明 pi extension upsert、tmux 可写、非 tmux 只读观察、server 不可达时 unbound 的用户可见行为。

## 自动化覆盖到的端到端路径

- managed pi runtime：`tests/pi_runtime_basic.rs`、`tests/pi_planner_and_hooks.rs`、`tests/pi_tmux_runtime_smoke.rs` 覆盖 pontia 启动 tmux-backed pi runtime、`session.ready`、current-turn context、Web submit、interrupt、restart/lifecycle 相关行为。
- 用户 tmux 中手动打开 pi：`tests/runtime_binding_upsert_api.rs` 覆盖 tmux upsert 记录 `tmux_socket_path + tmux_pane_id`、capability 可写；`clients/pi/test/attach.test.ts` 覆盖 extension 采集 tmux metadata 并调用 upsert。
- 非 tmux 普通终端打开 pi：`tests/runtime_binding_upsert_api.rs` 覆盖 non-tmux upsert 后 `accept_task=false`、`interrupt=false` 但 session 可观察；dashboard tests/check 覆盖 capability gating。
- Web submit / interrupt：`tests/pi_runtime_basic.rs`、`tests/application::turns` 单元测试和 `tests/runtime_interrupt.rs` 覆盖 pane binding dispatch/interrupt。
- TUI direct input：`clients/pi/test/index.test.ts` 覆盖 auto-upsert 后 current-turn 缺失时由 `agent_start` 创建 turn。
- raw transcript / timeline：`tests/raw_transcript_api.rs`、`tests/raw_transcript_backend.rs` 覆盖 `agent_bindings` 和 pi JSONL timeline 解析。
- artifacts 展示：`tests/artifact_discovery_api.rs`、`tests/artifact_content_api.rs` 覆盖 artifact discovery/content 外部 API。

## 验证命令

已通过：

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
pnpm --dir clients/pi test
pnpm --dir clients/pi typecheck
pnpm --dir=apps/dashboard run check
pnpm --dir=apps/dashboard run build
```

## 未自动化的手工项 / 风险

- 本次未启动真实交互式 pi TUI 做人工三场景验收；依赖现有 tmux smoke、upsert API、pi extension vitest 和 dashboard check/build 作为回归覆盖。
- tmux pane 失效、server 不可达 unbound、用户手动 pane restart 限制仍是设计中的运行时风险；当前行为由 capability、upsert 失败 unbound、pane liveness checks 和 restart start_command 优先级测试覆盖基础路径。
