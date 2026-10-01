## What Pontia is

Pontia is a console and control plane for coding agents. Agents continue to run on your machine, using your local development tools and project files, while you can view and control the same agent session from the terminal, local dashboard, or remote dashboard.

Pontia aims to provide:

- **Agent dashboard** — a friendly interface for working with your agents.
- **One session, control from anywhere** — keep the agent TUI and dashboard in sync, and continue the conversation from any device.
- **Remote access** — access agents running on your machine from anywhere.
- **Visible long-running tasks** — observable, dynamic workflows that break large tasks into manageable steps.

## Current status

Pontia currently supports:

- the pi coding agent;
- viewing and controlling agents from the web dashboard;
- two-way synchronization between the TUI and dashboard;
- remote access.

> Pontia is still under active development.

## Quick start

### Install

Run the installation script on Linux x86_64 or ARM64:

```bash
curl -fsSL https://pontia.dev/install.sh | sh
```

The script installs Pontia to `$HOME/.local/bin` by default. Make sure this directory is included in your `PATH`.

### First-time setup

Run the interactive initializer:

```bash
pontia init
```

Accept the default pi integration and follow the prompts. The initializer installs the pi plugin, configures Pontia, starts the background service, and opens the local dashboard.

Exiting the initializer does not stop Pontia.

### Start and stop

After setup, use the following commands to manage Pontia:

```bash
pontia up
pontia status
pontia down
```

The local dashboard is available at:

```text
http://127.0.0.1:8080/dashboard
```

Use the access token configured during setup when prompted.

## Remote access

Pontia can connect your local Pontia service to the public dashboard through an official edge. Your agents, development tools, and project files remain on your machine.

First, sign in to Pontia:

```bash
pontia login
```

Then register your machine and enable remote access:

```bash
pontia remote enable
```

Check the service status with:

```bash
pontia status
```

## Configuration

Pontia stores its configuration in `$HOME/.pontia/config.toml` by default. To use another directory, set `PONTIA_HOME` to an absolute path.

See [`.env.example`](.env.example) for available environment settings. Pontia does not automatically load `.env` files.

## License

Licensed under the [Apache License 2.0](LICENSE).
