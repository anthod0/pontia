# Bot conversations and tools

In the Dashboard sidebar, select **Create Bot**. Enter a name and an optional description, choose Pi, and enter a model ID available in that Pi installation. Each Bot has its own conversation. You can send another message while a Bot is running.

Use **Retry** on a failed message to send it again. You can keep sending new messages while a failed message remains in the chat. Edits to the client or model take effect when the Bot starts a new Session.

Bots start a fresh Session when their context exceeds the smaller of 258,000 tokens and 80% of the model's context window, after the current Session becomes idle. Above 75,000 tokens, five minutes of continuous idle ends the current Session; the next user message or helper report starts a new one. Helpers continue their work across this change. The new Session receives user information, Workspaces, recent memories, active helpers, and recent visible chat. Historical messages are not resent as tasks.

Configure the upper thresholds in `$PONTIA_HOME/config.toml`:

```toml
[bot]
context_token_limit = 258000
context_window_percent = 80
```

The token limit must be positive; the percentage must be between 1 and 100. Context-based handoff requires the client to report context usage.

Run the commands below inside a Bot Session. Pass values through named arguments. Successful commands print plain text. Add `--json` for a JSON result.

## Send a reply

```bash
pontia bot send-to-user --message 'I found the source of the error.' --working-on 'Checking the remaining cases'
```

The Dashboard shows the complete reply at once. Include `--working-on` to show ongoing work. Omit it to clear the working status.

## Update user information

```bash
pontia bot update-user --old-text '语言：中文' --new-text '语言：英文'
```

User information is shared by all Bots on the device in `$PONTIA_HOME/bot/USER.md` (default `~/.pontia/bot/USER.md`). Keep explicitly stated, lasting preferences and identity information here.

- `--old-text` must match exactly once. Matching is case-sensitive and preserves whitespace.
- Empty `--old-text` appends `--new-text`; empty `--new-text` deletes the match.
- A missing file starts empty. The result may contain at most 2,000 Unicode characters.
- Missing or multiple matches, including overlapping matches, and overlong results leave the file unchanged. The error includes the current text so the Bot can retry.
- Success confirms the update without returning the text. Each update uses the latest contents, preserving other Bots' changes to different text.

## Register a Workspace

```bash
pontia bot workspace register --root-id projects --path my-app --description '用户的网站项目'
```

`--root-id` selects a configured Workspace root. `--path` is relative to that root; the directory must already exist within it. Omitting `--path` registers the root directory itself. Optional `--name` sets the display name. Registering the same canonical directory reuses its Workspace ID and activates it.

Optional `--description` sets the Workspace description. Omitting it preserves any existing description. Descriptions may contain at most 2,000 Unicode characters and appear in the existing Workspace queries.

## Change a Workspace description

```bash
pontia bot workspace describe --workspace-id '<workspace-id>' --description '项目用途与背景'
```

Use `--clear-description` instead of `--description` to clear the description. One of these options is required. Changing it preserves the Workspace ID and activation state.

## Work with helpers

Assign work from a Bot Session:

```bash
pontia bot helper create --description 'Review the checkout' --message 'Review /path/to/project and report the findings.'
pontia bot helper list
pontia bot helper send --session-id '<helper-session-id>' --message 'Also check the migration.'
pontia bot helper check --session-id '<helper-session-id>' --limit 10
pontia bot helper close --session-id '<helper-session-id>'
```

Creation uses the Bot's default helper Profile. Choose another registered Profile with `--profile-id` and optionally `--profile-version`. The selected Profile supplies the client and model. Pass the working directory and task details in the assignment.

A helper reports to its Bot with `pontia bot helper reply --message '…'`. Reports reach the Bot's current Session, including after its previous Session exited. Helpers communicate through their Bot.

Sending returns `accepted`, `failed`, or `unknown`. Acceptance means the client accepted the input; it does not confirm execution or completion. Original messages are not automatically resent. Failed or uncertain helper reports also produce a system notification for the Bot, retained until it can be delivered.

Checking reads existing state and paginated native messages without waking or interrupting the helper. Tools are excluded by default; select them with `--category tool_call --category tool_result`. Use the returned continuation command for further pages. If native history is unavailable, the result still includes the Session state and identifies the history as unavailable.

Idle helpers remain available for more work. Closing requests Session exit and releases its runtime resources; after exit is confirmed, it disappears from the active helper list while its history remains available. Exit alone does not establish task failure or termination of a client's shared process.
