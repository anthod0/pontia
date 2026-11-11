<div align="center">
  <img src="assets/brand/logo-idle.svg" width="120" alt="Pontia logo">
  <h1>Pontia</h1>
  <p><strong>Your device. Your agent cloud.</strong></p>
  <p><a href="https://pontia.dev">🌐 Website</a> · <a href="https://app.pontia.dev">🖥️ Dashboard</a></p>
</div>

Pontia is a remote control plane for coding agents such as Claude Code, Codex, pi, and OpenCode. Your agents, development tools, and project files stay on your device while you view and control agent sessions from a web dashboard wherever you are.

![Pontia Dashboard showing recent sessions, a coding conversation, tool execution, and test results.](https://files.pontia.dev/cloud/dashboard-1280-9a7dd255904a.webp)

Pontia provides:

- 🎮 **Remote control** — create, view, continue, interrupt, and manage agent sessions remotely.
- 🔒 **End-to-end encrypted access** — traffic between the remote dashboard and your device is encrypted end to end.
- 🛰️ **Your choice of relay** — use an official Pontia Edge or deploy your own Edge for remote connectivity.
- 💻 **Local-first execution** — agents continue to run with the tools and files on your own device; the cloud provides access, not the execution environment.
- 🔄 **Real-time session synchronization** — move between the agent TUI, local dashboard, and remote dashboard without starting over.
- 📊 **Observable long-running tasks** — monitor and control dynamic Workflows that break large tasks into manageable steps.

## Current status

Pontia currently supports:

- pi, Codex, and Claude Code (through the Claude Agent client);
- ACP agents such as OpenCode, Kimi CLI, and Grok Build;
- local and remote web dashboards;
- end-to-end encrypted remote access through official or self-hosted Edges;
- two-way session control and real-time synchronization between clients and the dashboard;
- multi-step Workflows with history, pause, resume, retry, and replanning support.

## Quick start

### Requirements

- x86_64 and ARM64 Linux, or macOS 13.5 and later on Intel and Apple Silicon.
- a systemd user service manager on Linux; a per-user LaunchAgent on macOS.
- `curl`, `jq`, an OpenSSL implementation with Ed25519 support, and standard platform command-line tools.

Install the clients you plan to use before initialization:

- **pi:** the `pi` CLI and `tmux`;
- **Codex:** the `codex` CLI;
- **Claude Code:** Node.js LTS with npm, and a signed-in Claude Code. See the [Claude Agent guide](docs/claude-agent.md);
- **ACP agents:** the agent itself, enabled after initialization as described in [Agent Clients](https://pontia.dev/docs/agent-clients#enable-an-acp-agent).

### Install

Run the installation script:

```bash
curl -fsSL https://get.pontia.dev/install.sh | sh
```

The script installs Pontia to `$HOME/.local/bin` by default. Make sure this directory is included in your `PATH`.

### Initialize Pontia

```bash
pontia init
```

The initializer lets you select the pi, Codex, and Claude Agent integrations, starts the Pontia service, and offers to sign in to Pontia Cloud and register the device for remote access. Accept the remote access prompt and complete the browser authorization; Pontia opens the Remote Dashboard when setup finishes.

To install or repair one integration later, run `pontia setup` and select a client, or name it directly:

```bash
pontia setup claude-agent
```

## Configuration and management

### Start and stop

After setup, use the following commands to manage Pontia:

```bash
pontia up
pontia status
pontia down
```

### Remote access

Pontia connects your local service to the public dashboard through an official or self-hosted Edge. Dashboard traffic between the browser and your device is end-to-end encrypted, and the Edge relays only ciphertext. Your agents, development tools, and project files remain on your device.

`pontia init` sets up remote access when you accept its prompt. To connect an existing installation instead, first sign in to Pontia:

```bash
pontia login
```

Then register the current device and enable remote access:

```bash
pontia remote enable
```

Once connected, [open the remote dashboard](https://app.pontia.dev). To manage your Pontia Cloud account, [open settings](https://pontia.dev/settings).

Check the service status with:

```bash
pontia status
```

### Local access

Open the local dashboard on the device running Pontia. The default address is:

<http://127.0.0.1:8080/dashboard>

`pontia init` automatically generates an access token. To view it, check `external_api_token` in `$HOME/.pontia/config.toml`.

### Updates

Update the CLI and background service to the latest signed stable release:

```bash
pontia update
```

### Configuration

Pontia stores its configuration in `$HOME/.pontia/config.toml` by default. To use another directory, set `PONTIA_HOME` to an absolute path before running the initializer and subsequent commands.

## License

Pontia is licensed under the [Apache License 2.0](LICENSE).
