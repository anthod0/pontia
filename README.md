<div align="center">
  <img src="assets/brand/logo-idle.svg" width="64" alt="Pontia">
  <h1>Pontia</h1>
</div>

Pontia gives you a Lead Bot that coordinates coding agents on your own machines from a desktop or mobile browser.

[pontia.dev](https://pontia.dev) · [Quick start](#quick-start) · [Dashboard](https://app.pontia.dev) · [简体中文](README.zh-CN.md)

![Lead Bot chat with two helpers, a delivered report and pending decisions](https://files.pontia.dev/cloud/lead-desk-2640-f11325427ccd.webp)

## Features

- **Hear from the Lead Bot when it needs you.** The Lead Bot brings decisions and approval requests to your Todo.
- **Step into any native session.** Open any helper’s running CLI session to see what it’s doing or take over.
- **Access your local machine.** Securely preview local files, HTML prototypes, and localhost dev servers in a desktop or mobile browser.
- **Use the right model for each role.** Choose a model and one of six supported CLIs for each role in its Agent Profile.
- **Carry your preferences across sessions.** Bots on the same device recall past decisions and follow the preferences you keep in `USER.md`.
- **Connect securely to every device.** Reach any registered machine from anywhere over a direct, end-to-end encrypted connection from your browser.

## Supported clients

- **During setup** — Claude Code, Codex and Pi (requires `tmux`).
- **After setup** — Enable Google Antigravity, Kiro or Grok Build under **Settings → Agent Clients**. [Client setup guide](https://pontia.dev/docs/agent-clients)

## Quick start

1. On Linux with systemd user services or macOS 13.5+, install Node.js LTS, npm and the coding CLI you want to use.
2. Run the setup wizard on that machine. Follow the prompts to sign in and register the device.

   ```sh
   npx @pontia/cli init
   ```

3. Open the [Dashboard](https://app.pontia.dev), select your device and give your Lead Bot its first task.

## Screenshots

**1. TODO** — Answer a question from your Bot in the sidebar.

![Juniper asks which trail name to print, with a reply drafted below](https://files.pontia.dev/cloud/sidebar-todo-1120-91689148538f.webp)

**2. Context** — Follow the messages behind a decision.

![The original trail question, survey findings and selected naming decision](https://files.pontia.dev/cloud/sidebar-context-1120-9e2bed519e8c.webp)

**3. Preview** — Open a local dev server alongside the chat.

![The local Luma field guide showing the Cedar Spur route](https://files.pontia.dev/cloud/sidebar-preview-1120-dc9f6355c8d9.webp)

**4. Files** — Open a CSV file from chat to review it as a table.

![Trail survey table showing Cedar Spur on the trail sign and North Fork in the survey](https://files.pontia.dev/cloud/sidebar-files-1120-a0085ef90962.webp)

**5. Git** — Review staged, unstaged and untracked files in a workspace.

![Git changes on main grouped by staging status](https://files.pontia.dev/cloud/sidebar-git-1120-bf16b149ad76.webp)

## Documentation

[Installation guide](https://pontia.dev/docs/getting-started) · [CLI reference](https://pontia.dev/docs/cli) · [Security model](https://pontia.dev/docs/security) · [Self-host an Edge](https://pontia.dev/docs/self-hosted-edge)

## License

[Apache License 2.0](LICENSE)
