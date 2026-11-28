<div align="center">
  <img src="assets/brand/logo-idle.svg" width="64" alt="Pontia">
  <h1>Pontia</h1>
</div>

Pontia 让你从电脑或手机浏览器给 Lead Bot 派任务，由它协调开发机上的编程助手一起完成。

🚀 [快速开始](#快速开始) · 🌐 [主页](https://pontia.dev) · 🖥️ [Dashboard](https://app.pontia.dev) · 🌍 [English](README.md)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://files.pontia.dev/cloud/hero-window-dark-676bf36f6898.png">
  <img src="https://files.pontia.dev/cloud/hero-window-light-25250a18ba8f.png" alt="Lead Bot 向 12 名 Helper 分派任务，对话右侧显示设置页预览">
</picture>

## 复用现有订阅（BYOS）

Pontia 直接在本机运行受支持的官方 CLI 和 Claude Agent SDK，复用你已有的订阅、API Key 或自定义端点，不提取或转发 OAuth Token。

## 主要功能

- **需要的时候才提醒你**：Lead Bot 只把需要你决定或确认的事项放进待办。
- **随时介入原生会话**：打开任一 Helper 正在运行的原生 CLI 会话，查看进展或直接接管。
- **轻松访问本机**：在电脑或手机浏览器中安全预览本机文件、HTML 原型和 localhost 开发服务。
- **使用最合适的模型**：通过 Agent Profile 为每个角色选择合适的模型，并从六种受支持的 CLI 中选用一种。
- **记住你的事情**：同一台设备上的 Bot 能跨会话回顾以往的决定，并遵循你写在 `USER.md` 里的偏好。
- **每台设备都能加密直连**：无论身在何处，都能从浏览器通过端到端加密连接直达任一已注册的开发机。

## 支持的客户端

- **Claude Code**
- **Codex**
- **Grok Build**
- **Kiro**
- **Google Antigravity**
- **Pi**

配置方法见[客户端配置指南](https://pontia.dev/zh/docs/agent-clients)。

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
