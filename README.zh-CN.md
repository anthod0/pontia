<div align="center">
  <img src="assets/brand/logo-idle.svg" width="64" alt="Pontia">
  <h1>Pontia</h1>
</div>

Pontia 让你从电脑或手机浏览器给 Lead Bot 派任务，由它协调开发机上的编程助手一起完成。

🚀 [快速开始](#快速开始) · 🌐 [主页](https://pontia.dev) · 🖥️ [Dashboard](https://app.pontia.dev) · 🌍 [English](README.md)

![Lead Bot 对话中显示两名 Helper 的分工、交回的报告和待确认的问题](https://files.pontia.dev/cloud/lead-desk-2640-f11325427ccd.webp)

## 复用现有订阅（BYOS）

Pontia 直接在你的设备上运行受支持的官方 CLI 进程；Claude Code 则通过官方 Agent SDK 运行。它沿用这些客户端已有的登录状态与提供商设置，因此 Claude、ChatGPT/Codex、Grok、Kiro、Antigravity 等订阅，以及 Pi 提供商凭据、API Key 和自定义模型端点，都留在对应的原生客户端中。Pontia 不提取或转发 OAuth Token。

## 主要功能

- **需要的时候才提醒你**：Lead Bot 只把需要你决定或确认的事项放进待办。
- **随时介入原生会话**：打开任一 Helper 正在运行的原生 CLI 会话，查看进展或直接接管。
- **轻松访问本机**：在电脑或手机浏览器中安全预览本机文件、HTML 原型和 localhost 开发服务。
- **使用最合适的模型**：通过 Agent Profile 为每个角色选择合适的模型，并从六种受支持的 CLI 中选用一种。
- **记住你的事情**：同一台设备上的 Bot 能跨会话回顾以往的决定，并遵循你写在 `USER.md` 里的偏好。
- **每台设备都能加密直连**：无论身在何处，都能从浏览器通过端到端加密连接直达任一已注册的开发机。

## 支持的客户端

- **初始化时接入**：Claude Code、Codex、Pi（需安装 `tmux`）。
- **初始化后启用**：Google Antigravity、Kiro、Grok Build，入口在「设置 → Agent 客户端」。[客户端配置指南](https://pontia.dev/zh/docs/agent-clients)

## Agent 能力矩阵

| Client | 接入方式 | Session 控制 | 审批模式 | 模型与推理强度 | 工作目录 |
| --- | --- | --- | --- | --- | --- |
| Pi | 原生 TUI 插件 | 恢复、引导、中断、时间线分支 | 默认、受限、YOLO | 客户端模型目录及其公布的推理强度 | 每个 Session 使用一个 Workspace；并行写任务使用独立 Git worktree |
| Codex | 原生 app-server | 恢复、引导、中断、将较早提示词编辑为新 Session | 默认、受限、YOLO | 客户端模型目录及其公布的推理强度 | 每个 Session 使用一个 Workspace；并行写任务使用独立 Git worktree |
| Claude Code | Claude Agent SDK | 恢复、引导、中断、将较早提示词编辑为新 Session | 默认、受限、YOLO | 客户端模型目录及其公布的推理强度 | 每个 Session 使用一个 Workspace；并行写任务使用独立 Git worktree |
| Grok Build | ACP server | 恢复、插入指令、中断、在客户端声明支持时压缩上下文 | 默认、受限、YOLO | ACP 模型目录及其公布的推理强度 | 每个 Session 使用一个 Workspace；并行写任务使用独立 Git worktree |
| Google Antigravity | ACP server | 恢复、引导、中断 | 默认、受限、YOLO | ACP 模型目录及其公布的推理强度 | 每个 Session 使用一个 Workspace；并行写任务使用独立 Git worktree |
| Kiro | ACP server | 恢复、在客户端声明支持时引导和压缩上下文、中断 | 默认、受限、YOLO | ACP 模型目录及其公布的推理强度 | 每个 Session 使用一个 Workspace；并行写任务使用独立 Git worktree |

## 使用 Git worktree 并行推进任务

把多个独立任务交给 Lead Bot，它可以并行启动专注于单项工作的 Helper。[Agent 配置](https://pontia.dev/zh/docs/agent-profiles)可为每个角色指定客户端、模型、推理强度、权限和任务指令。

当多个 Helper 会写入同一仓库时，让每个 Helper 从原有活跃 Workspace 建立短期分支和独立 Git worktree。这样可以把并行修改隔离到不同 checkout，同时共享项目上下文。你可以打开任一 Helper 的原生 Session，查看工作内容和它报告的 Diff；也可以用 Dashboard 的 Files 与 Git 面板检查活跃 Workspace。各分支准备好后，再由 Lead Bot 协调验证与合入。

## 快速开始

1. 准备一台启用了 systemd 用户服务的 Linux 机器，或一台运行 macOS 13.5 及以上版本的 Mac。安装 Node.js LTS、npm 和要使用的编程 CLI。
2. 在这台机器上运行以下命令，按提示登录 Pontia 并注册设备。

   ```sh
   npx @pontia/cli init
   ```

3. 打开[控制台](https://app.pontia.dev)，选中刚注册的设备，给 Lead Bot 派发第一个任务。

## 界面截图

**1. TODO** — 直接在侧栏回复待确认的问题。

![Juniper 询问地图上应使用哪个地名，下方是正在编辑的回复](https://files.pontia.dev/cloud/sidebar-todo-1120-91689148538f.webp)

**2. Context** — 查看一项决定的来龙去脉。

![最初的路线问题、调查结果，以及当前选中的命名决策](https://files.pontia.dev/cloud/sidebar-context-1120-9e2bed519e8c.webp)

**3. Preview** — 在聊天侧栏打开本地开发服务。

![本地运行的 Luma 指南展示 Cedar Spur 路线](https://files.pontia.dev/cloud/sidebar-preview-1120-dc9f6355c8d9.webp)

**4. Files** — 浏览工作区文件，预览代码、Markdown、HTML、JSON/YAML、CSV/TSV 表格和图片。

![路线调查表显示，12 号路口的路牌写着 Cedar Spur，调查记录中则是 North Fork](https://files.pontia.dev/cloud/sidebar-files-1120-a0085ef90962.webp)

**5. Git** — 查看工作区中已暂存、未暂存和未跟踪的文件。

![Git 侧栏按暂存状态展示 main 分支上的文件改动](https://files.pontia.dev/cloud/sidebar-git-1120-bf16b149ad76.webp)

## 文档

[安装指南](https://pontia.dev/zh/docs/getting-started) · [CLI 命令参考](https://pontia.dev/zh/docs/cli) · [安全模型](https://pontia.dev/zh/docs/security) · [自托管 Edge](https://pontia.dev/zh/docs/self-hosted-edge)

## 许可证

[Apache License 2.0](LICENSE)
