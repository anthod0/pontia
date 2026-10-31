# Claude Agent

Claude Agent lets you run Claude coding tasks from Pontia's Dashboard or External API. You can send text tasks, watch output, approve or deny tools, interrupt execution, and browse or resume the same conversation after exiting.

## Set up

Use Linux or macOS 13.5 or later, with Node.js LTS available on your `PATH`. Install an official Pontia release, then configure authentication through Claude Code under the same operating-system user that runs Pontia.

Run:

```sh
pontia init
```

Select `claude-agent`. Initialization checks the runtime dependencies before enabling the integration. Pontia does not provide a Claude login screen or save or refresh Claude credentials.

If you use a custom native configuration directory, pass its absolute path during initialization:

```sh
CLAUDE_CONFIG_DIR=/absolute/path/to/claude-config pontia init
```

## Run a task

In Dashboard, choose a Workspace, create a **Claude Agent** Session, and send your first task. Subsequent tasks wait in the Inbox until the current task has a confirmed result.

Tool requests offer **Allow once** and **Deny once**. These choices do not grant permanent permission. Interrupt requests cancellation of the current task; Exit closes the Session's execution. After exiting, you can still browse its history and send another message to resume the same conversation.

Claude Agent does not attach to the native Claude Code terminal interface. Model switching, manual compaction, branching, and question forms are not available in this integration. Continuing active execution across a Pontia service restart or update is not guaranteed.

## Resolve setup and authentication problems

If dependency checks fail, ensure Node.js LTS is available and reinstall the official Pontia release. A successful dependency check does not establish that your Claude credentials or model provider work.

If Dashboard reports an authentication failure while the SDK retries, check Claude Code's native authentication or provider configuration for the service user. Pontia preserves the SDK's retry behavior; a retry notification is not a completed or failed task. You can exit the Session while it is retrying.
