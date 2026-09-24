# Codex daemon integration

Pontia connects to Codex's shared native daemon on Linux. During `pontia init`, select Codex to register and enable `pontia-codex.service`, enable user linger, start the daemon through the native command, and verify a real control-protocol connection.

Initialization uses the Codex executable found on `PATH` and `${CODEX_HOME:-$HOME/.codex}`. The same absolute `CODEX_HOME` is written to both the Codex startup service and `pontia.service`. It does not run `codex login`, modify Codex configuration or credentials, or send a model request.

The startup service runs:

```sh
codex app-server daemon start
```

The command is idempotent and reuses an already-running shared daemon. `pontia down` stops only Pontia; it does not stop the Codex daemon or disable `pontia-codex.service`.

Pontia creates threads with legacy history because Codex 0.156.1 does not support the resume and history operations needed by this integration for paginated threads. Existing external threads must support those operations before Pontia can restore control.

External clients can operate a thread already bound to a Pontia Session. Pontia subscribes and reconciles its lifecycle independently of which client sent the input. Historical threads and native sub-agents are not automatically imported. Use the Pontia-associated TUI to switch to another thread and associate its Session; Dashboard view following requires that TUI connection's evidence.

Approvals and questions remain in the native TUI. Closing Pontia releases its connections and TUI gateways while the daemon continues running. Exiting an individual Session archives its thread. After a daemon restart, Pontia verifies the new instance and reconciles native state before reopening control; input with an unknown delivery result is not automatically resent.

The opt-in integration test uses an already running daemon and model access, creates a dedicated thread in a temporary workspace, and archives that thread:

```sh
cargo test -p pontia-client-codex external_daemon_two_clients -- --ignored --nocapture
```

To also test daemon replacement, set `PONTIA_CODEX_TEST_WAIT_FOR_RESTART=1` for this command. When it prints `READY_FOR_EXTERNAL_DAEMON_RESTART`, restart the test daemon from another terminal within two minutes. The test itself never manages the daemon.
