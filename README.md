<p align="center">Keep your coding agents working beyond a single terminal window.</p>

> Pontia is experimental and under active development. Some workflows are incomplete, and breaking changes should be expected.

## What Pontia is

Pontia is for developers who want coding agents to keep working beyond one terminal window.

It aims to provide:

- **Persistent agent sessions** — keep working with your agent over time without giving up its familiar terminal experience.
- **One session, control from anywhere** — start, continue, observe, or steer the same agent session from your terminal or web dashboard, with broader desktop and mobile access as a product goal.
- **Visible long-running tasks** — let agents break large tasks into manageable steps so you can understand progress, intervene, and retry work when needed.

In short: Pontia keeps agent work alive, visible, and under your control.

### Long-running work you can follow

Long tasks should not be opaque prompts that run for hours with no structure.

Pontia's goal is to let agents turn a large task into a clear plan: what needs to happen, which steps depend on others, and what is ready to work on next. As work progresses, agents should be able to adjust the plan rather than blindly follow it.

Developers should be able to inspect each step, understand the results, and intervene, retry, or revise part of the task without starting everything over.

This is a product direction, not a capability of the current release.

## Current status

Pontia is currently intended for local development use. It supports:

- pi as the supported coding agent;
- session creation, conversation, termination, and resume;
- a web dashboard for viewing and controlling sessions;
- interaction with the same session from the terminal and the web.

## Roadmap

- [x] pi integration
- [x] Basic web dashboard
- [x] Session creation, conversation, termination, and resume
- [x] Terminal and web control of the same session
- [ ] Human approval and review workflows
- [ ] Agent-created plans for large tasks
- [ ] Long-running task scheduling, progress inspection, retry, and repair
- [ ] Stable product documentation
- [ ] More coding agent integrations

## Get started

### Install

1. Download the `pontia` and `pontiad` packages for your operating system and processor from [Releases](https://github.com/anthod0/pontia/releases/latest).
2. Extract both executables into the same directory on your `PATH`, such as `$HOME/.local/bin`.
3. Install pi CLI and tmux if they are not already available.

Pontia supports Linux with systemd and macOS. No Rust, Cargo, or frontend build tools are needed to use the published binaries.

### First-time setup

Run the interactive initializer:

```bash
pontia init
```

Accept the default pi integration and follow the prompts. The initializer installs the pi plugin, configures Pontia, starts the service, and opens the dashboard.

Exiting the initializer does not stop Pontia.

### Start and stop

After setup, use:

```bash
pontia up
pontia status
pontia down
```

The dashboard is available at `http://127.0.0.1:8080/dashboard` by default. Use the access token configured during setup when prompted.

### Configuration

Pontia stores its configuration in `$HOME/.pontia/config.toml` by default. To use another location, set `PONTIA_HOME` to an absolute directory path.

See [`.env.example`](.env.example) for available environment settings. Pontia does not automatically load `.env` files.

## License

Licensed under the [Apache License 2.0](LICENSE).
