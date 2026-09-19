# @pontia/pi-client-plugin

Connect your pi coding-agent sessions to Pontia so you can follow and continue your work from the web dashboard.

## Requirements

- [Pontia](https://pontia.dev/) running locally
- pi CLI
- tmux

## Install

If you selected the pi integration during `pontia init`, the plugin is already installed.

To install it separately:

```bash
pi install npm:@pontia/pi-client-plugin
```

The plugin loads automatically when pi starts.

## Get started

Open the Pontia dashboard and create a pi session. You can then view the session and continue the conversation from the dashboard.

You can also start pi in a tmux pane inside a workspace registered with Pontia.

For Pontia installation and setup, see the [getting started guide](../../README.md#get-started).

## Troubleshooting

If a session does not appear in the dashboard, check that:

- Pontia is running (`pontia status`).
- The workspace is registered with Pontia.
- pi is running inside tmux.
- If you just installed the plugin, you have restarted pi.

## License

[Apache License 2.0](LICENSE)
