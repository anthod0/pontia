<div align="center">
  <img src="assets/brand/logo-idle.svg" width="120" alt="Pontia logo">
  <h1>Pontia</h1>
  <p><strong>Your device. Your agent cloud.</strong></p>
  <p>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="License: Apache-2.0"></a>
    <a href="https://www.npmjs.com/package/@pontia/cli"><img src="https://img.shields.io/npm/v/@pontia/cli" alt="npm"></a>
    <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS-lightgrey" alt="Platform: Linux | macOS">
  </p>
  <p><a href="https://pontia.dev">🌐 Website</a> · <a href="https://app.pontia.dev">🖥️ Dashboard</a></p>
  <p>English · <a href="README.zh-CN.md">简体中文</a></p>
</div>

Leave your desk, not your agents. Pontia lets you run coding agents such as pi, Codex, Claude Code, and OpenCode on your own device, then check in, approve, and steer every session from any browser, including your phone. Pontia is free and open source.

<p align="center">
  <img src="apps/cloud/static/client-logos/pi.svg" height="28" alt="pi">
  &nbsp;&nbsp;
  <picture><source media="(prefers-color-scheme: dark)" srcset="assets/clients/codex-dark.svg"><img src="apps/cloud/static/client-logos/codex.svg" height="28" alt="Codex"></picture>
  &nbsp;&nbsp;
  <img src="apps/cloud/static/client-logos/claude-code.svg" height="28" alt="Claude Code">
  &nbsp;&nbsp;
  <picture><source media="(prefers-color-scheme: dark)" srcset="apps/cloud/static/client-logos/opencode-dark.svg"><img src="apps/cloud/static/client-logos/opencode.svg" height="28" alt="OpenCode"></picture>
  &nbsp;&nbsp;
  <img src="apps/cloud/static/client-logos/grok-build.svg" height="28" alt="Grok Build">
</p>

![Pontia Dashboard showing recent sessions, a coding conversation, tool execution, and test results.](https://files.pontia.dev/cloud/dashboard-1280-9a7dd255904a.webp)

## Why Pontia

- 📱 **In your pocket** — add the Dashboard to your phone's home screen. Follow progress, answer approvals, and send the next instruction away from your desk.
- 💳 **Bring your own subscriptions** — Pontia doesn't sell models or tokens. Your agents run on the accounts and plans you already pay for.
- 💻 **Keep your terminal** — agents run on your device with your tools and files. Keep using the pi or Codex TUI; conversations sync to the Dashboard automatically, so you can switch between them without starting over.
- 🔒 **Encrypted end to end** — only your browser and your device can read a session. Use an official Edge or run your own; either way, the Edge only relays ciphertext.

Pontia supports pi, Codex, Claude Code, and ACP agents such as OpenCode and Grok Build, on Linux and macOS.

## Quick start

### Requirements

- x86_64 and ARM64 Linux, or macOS 13.5 and later on Intel and Apple Silicon.
- a systemd user service manager on Linux; a per-user LaunchAgent on macOS.
- Node.js with npm for the recommended `npx` command. Without Node.js, use the [shell installer](#install-with-the-shell-script).

Install the clients you plan to use before initialization:

- **pi:** the `pi` CLI and `tmux`;
- **Codex:** the `codex` CLI;
- **Claude Code:** Node.js LTS with npm, and a signed-in Claude Code. See the [Claude Code guide](docs/claude-agent.md);
- **ACP agents:** the agent itself, enabled after initialization as described in [Agent Clients](https://pontia.dev/docs/agent-clients#enable-an-acp-agent).

### Initialize

```bash
npx @pontia/cli init
```

This one command completes the whole setup:

1. Installs Pontia to `$HOME/.local/bin`. Add that directory to your `PATH`.
2. Lets you select clients and installs their integrations.
3. Starts the Pontia service.
4. Signs you in to Pontia Cloud and registers the device for remote access.

Pontia opens the [remote dashboard](https://app.pontia.dev) when setup succeeds. If Pontia is already installed, run `pontia init` directly. Rerunning it checks and repairs the installation while keeping your existing configuration.

### Common commands

Check whether Pontia is working:

```bash
pontia status
```

It shows the service health, the signed-in user, and the remote registration and edge of this device.

Configure or repair a client connection, for example after installing a new client or when a client stops connecting:

```bash
pontia setup          # select clients
pontia setup codex    # or name one: pi, codex, claude
```

When something is wrong, run a detailed check:

```bash
pontia doctor
```

`pontia doctor` checks the service, configuration, login, and remote registration. Each failed check includes a repair hint. For a registered device, the report also shows its remote dashboard address. See [Troubleshooting](https://pontia.dev/docs/troubleshooting) for more help.

### Install with the shell script

Without Node.js, use the signed release installer. It requires `curl`, an OpenSSL implementation with Ed25519 support, and standard platform command-line tools.

```bash
curl -fsSL https://get.pontia.dev/install.sh | sh
"$HOME/.local/bin/pontia" init
```

## Manual configuration

`pontia init` covers everything above. Use the following commands to run one step on its own, or to set up without prompts from a script or agent. See [Initialization](https://pontia.dev/docs/initialization#non-interactive-initialization) for the full step-by-step sequence and the [CLI reference](https://pontia.dev/docs/cli) for all commands.

| Command | Purpose |
| --- | --- |
| `pontia up` / `pontia down` | Start or stop the Pontia service |
| `pontia login` | Sign in to Pontia Cloud |
| `pontia remote enable` | Register this device and enable remote access |
| `pontia update` | Update the CLI and service to the latest signed stable release |

### Shell completion

Add the line for your shell, then open a new terminal:

```bash
# bash: ~/.bashrc
source <(pontia completions bash)

# zsh: ~/.zshrc, after compinit
source <(pontia completions zsh)

# fish: run once
mkdir -p ~/.config/fish/completions
pontia completions fish > ~/.config/fish/completions/pontia.fish
```

Elvish and PowerShell are also supported. See the [CLI reference](https://pontia.dev/docs/cli).

### Local dashboard

Open the local dashboard on the device running Pontia at <http://127.0.0.1:8080/dashboard>. Its access token is `external_api_token` in `$HOME/.pontia/config.toml`, generated by `pontia init`.

### Configuration directory

Pontia stores its configuration in `$HOME/.pontia/config.toml` by default. To use another directory, set `PONTIA_HOME` to an absolute path before running `pontia init` and later commands.

## License

Pontia is licensed under the [Apache License 2.0](LICENSE).
