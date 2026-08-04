# @pontia/pi-client-plugin

First-party pi extension for connecting pi sessions to pontia.

## Requirements

- A locally running pontia server
- A workspace registered in pontia
- pi CLI
- A tmux pane (`TMUX` and `TMUX_PANE` must be available)
- `PONTIA_HOME` when pontia uses a location other than `$HOME/.pontia`

## Install locally

From the pontia repository root, register this package in pi's user-level settings:

```bash
pi install ./clients/pi
```

Pi records the local package path without copying it, so keep the repository path available while using the plugin. The user-level install makes the plugin available in every workspace; the extension remains a silent no-op outside a tmux pane or an active workspace registered in Pontia.

## Use with pontia

For sessions started by pontia, configure the pi command in `$PONTIA_HOME/config.toml`:

```toml
[runtime.pi]
tui_command = "pi"
```

Pontia appends the required `--approve` and native session identity arguments when it starts pi. The plugin is loaded from pi's user-level package settings rather than through a per-launch extension path.

Pontia supplies a Session hint when it starts pi. Pi exposes that hint as its native `client_session_key`; the extension uses this key to identify and bind the Pontia Session, and the backend returns the canonical Runtime Instance ID. Tmux pane markers are not a Session identity source.

A new manually started pi session inside tmux is not persisted in Pontia until its first prompt starts. An exited session with the same native key can reconnect, while a second TUI for a key already bound to a non-exited Session is ignored. After binding succeeds, Pontia writes `@pontia_session_id` and `@pontia_runtime_instance_id` to the pane for runtime management and clears both markers when the Session exits.

Regardless of where the start command came from, run pi in a tmux pane and in an active workspace registered in pontia. Outside tmux the extension is a silent no-op. If pontia is unavailable or the workspace is not registered, the extension leaves the pi session running without pontia reporting.

## Troubleshooting

The extension writes diagnostics to:

```text
${PONTIA_HOME:-$HOME/.pontia}/state/pi-hook.log
```

Follow the log while reproducing a problem:

```bash
tail -f "${PONTIA_HOME:-$HOME/.pontia}/state/pi-hook.log"
```

## Development

From the repository root:

```bash
pnpm --dir clients/pi test
pnpm --dir clients/pi typecheck
```
