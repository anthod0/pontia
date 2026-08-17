# @pontia/pi-client-plugin

Connect Pi coding-agent sessions to Pontia.

## Requirements

- [Pontia](https://pontia.dev/) running locally
- Pi CLI
- tmux

## Install

```bash
pi install npm:@pontia/pi-client-plugin
```

## Configure

Set the Pi command in `$PONTIA_HOME/config.toml`:

```toml
[runtime.pi]
tui_command = "pi"
```

Then launch Pi from Pontia, or run it in a tmux pane inside a workspace registered with Pontia. The extension loads automatically.

## Troubleshooting

If a session does not appear in Pontia, verify that:

- Pontia is running
- the current workspace is registered with Pontia
- Pi is running inside tmux

Extension logs are available at:

```text
${PONTIA_HOME:-$HOME/.pontia}/state/pi-hook.log
```

Follow the log while reproducing an issue:

```bash
tail -f "${PONTIA_HOME:-$HOME/.pontia}/state/pi-hook.log"
```

## License

[Apache License 2.0](LICENSE)
