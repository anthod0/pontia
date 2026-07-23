# @pontia/pi-client-plugin

First-party pi extension for connecting pi sessions to pontia.

## Requirements

- A locally running pontia server
- A workspace registered in pontia
- pi CLI
- `PONTIA_HOME` when pontia uses a location other than `$HOME/.pontia`

## Install locally

From the pontia repository root, run pi with this package as a temporary extension:

```bash
pi --approve -e ./clients/pi
```

Or install it into project-local pi settings:

```bash
pi install -l ./clients/pi
```

## Use with pontia

For sessions started by pontia, configure the pi command in `$PONTIA_HOME/config.toml`:

```toml
[runtime.pi]
tui_command = "pi -e /absolute/path/to/pontia/clients/pi"
```

Pontia supplies a Session hint when it starts pi. After pi exposes its real native session identity, the extension confirms the same runtime binding with Pontia; the backend then returns the canonical Runtime Instance ID.

A new manually started pi session is not persisted in Pontia until its first prompt starts. A manual session that already has an agent binding reconnects immediately.

Regardless of where the start command came from, run pi in an active workspace registered in pontia. If pontia is unavailable or the workspace is not registered, the extension leaves the pi session running without pontia reporting.

Web-based input is available when the confirmed runtime includes a writable tmux pane. Other sessions may remain observable without accepting input from the Web Dashboard.

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
