# Codex daemon integration

Pontia connects to an externally running Codex **0.156.1** daemon on Linux. Start it before using Codex Sessions:

```sh
codex app-server daemon start
codex app-server daemon version
```

Run Pontia as the same user, with the same `CODEX_HOME` as the daemon (default: `~/.codex`). The version check applies to the connected daemon, which can differ from the installed CLI. An unavailable or unsupported daemon leaves the control channel unavailable; existing Session and thread bindings remain intact.

Pontia creates threads with legacy history because Codex 0.156.1 does not support the resume and history operations needed by this integration for paginated threads. Existing external threads must support those operations before Pontia can restore control.

External clients can operate a thread already bound to a Pontia Session. Pontia subscribes and reconciles its lifecycle independently of which client sent the input. Historical threads and native sub-agents are not automatically imported. Use the Pontia-associated TUI to switch to another thread and associate its Session; Dashboard view following requires that TUI connection's evidence.

Approvals and questions remain in the native TUI. Closing Pontia releases its connections and TUI gateways while the daemon continues running. Exiting an individual Session archives its thread. After a daemon restart, Pontia verifies the new instance and reconciles native state before reopening control; input with an unknown delivery result is not automatically resent.

The opt-in integration test uses an already running daemon and model access, creates a dedicated thread in a temporary workspace, and archives that thread:

```sh
cargo test -p pontia-client-codex external_daemon_two_clients -- --ignored --nocapture
```

To also test daemon replacement, set `PONTIA_CODEX_TEST_WAIT_FOR_RESTART=1` for this command. When it prints `READY_FOR_EXTERNAL_DAEMON_RESTART`, restart the test daemon from another terminal within two minutes. The test itself never manages the daemon.
