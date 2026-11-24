# Pontia

Pontia gives you a Lead Bot that coordinates coding agents on your own machines from a desktop or mobile browser.

[Quick start](#quick-start) · [Dashboard](https://app.pontia.dev) · [简体中文](README.zh-CN.md)

![Lead Bot chat with two helpers, a delivered report and pending decisions](https://files.pontia.dev/cloud/lead-desk-2640-f11325427ccd.webp)

## Features

- **Parallel tasks.** Give the Lead Bot a project. It assigns work to helpers and collects their reports and files.
- **Decisions.** Review open questions in Todo and answer them individually.
- **Shared memory.** Bots on the same device can recall past decisions across sessions. Keep your standing instructions in `USER.md`.
- **Agent Profiles.** Save a role’s instructions, client and model for other helpers to use.
- **Remote access.** Switch between registered machines in the Dashboard. Each machine runs its own clients, with an end-to-end encrypted connection to your browser.
- **Local files and ports.** Securely preview files, HTML prototypes, and localhost dev servers from your browser or phone.

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

**Todo** — Review a helper’s question and send your decision.

![Todo queue with a trail-naming decision awaiting a reply](https://files.pontia.dev/cloud/helper-decisions-2640-303231730ebb.webp)

**Memory** — Look up a decision from an earlier session.

![Memory search showing a past trail decision](https://files.pontia.dev/cloud/shared-memory-2640-36fb6dde6684.webp)

## Documentation

[Installation guide](https://pontia.dev/docs/getting-started) · [CLI reference](https://pontia.dev/docs/cli) · [Security model](https://pontia.dev/docs/security) · [Self-host an Edge](https://pontia.dev/docs/self-hosted-edge)

## License

[Apache License 2.0](LICENSE)
