# Pontia

Pontia 让你从电脑或手机浏览器给 Lead Bot 派任务，由它协调开发机上的编程助手一起完成。

[快速开始](#快速开始) · [打开控制台](https://app.pontia.dev) · [English](README.md)

![Lead Bot 对话中显示两名 Helper 的分工、交回的报告和待确认的问题](https://files.pontia.dev/cloud/lead-desk-2640-f11325427ccd.webp)

## 主要功能

- **并行分工**：把项目交给 Lead Bot，它会拆分任务，安排 Helper 同时处理，再汇总报告和文件。
- **待办决策**：需要你决定的问题集中在 Todo 里，可以逐项回复。
- **共享记忆**：同一台设备上的 Bot 能查到以往会话中的决定。长期适用的个人偏好可以写进 `USER.md`。
- **角色配置**：把角色指令、客户端和模型存为 Agent Profile，其他 Helper 也能直接使用。
- **远程访问**：在控制台切换已注册的开发机。客户端在各自的机器上运行，浏览器与设备之间的连接采用端到端加密。

## 支持的客户端

- **初始化时接入**：Claude Code、Codex、Pi（需安装 `tmux`）。
- **初始化后启用**：Google Antigravity、Kiro、Grok Build，入口在「设置 → Agent 客户端」。[客户端配置指南](https://pontia.dev/zh/docs/agent-clients)

## 快速开始

1. 准备一台启用了 systemd 用户服务的 Linux 机器，或一台运行 macOS 13.5 及以上版本的 Mac。安装 Node.js LTS、npm 和要使用的编程 CLI。
2. 在这台机器上运行以下命令，按提示登录 Pontia 并注册设备。

   ```sh
   npx @pontia/cli init
   ```

3. 打开[控制台](https://app.pontia.dev)，选中刚注册的设备，给 Lead Bot 派发第一个任务。

## 界面截图

**待办决策**：查看 Helper 提出的问题，直接回复你的决定。

![Todo 队列中，一项路线命名问题正在等待回复](https://files.pontia.dev/cloud/helper-decisions-2640-303231730ebb.webp)

**共享记忆**：查找之前的会话里做过的决定。

![记忆搜索结果中显示了之前的路线决策记录](https://files.pontia.dev/cloud/shared-memory-2640-36fb6dde6684.webp)

## 文档

[安装指南](https://pontia.dev/zh/docs/getting-started) · [CLI 命令参考](https://pontia.dev/zh/docs/cli) · [安全模型](https://pontia.dev/zh/docs/security) · [自托管 Edge](https://pontia.dev/zh/docs/self-hosted-edge)

## 许可证

[Apache License 2.0](LICENSE)
