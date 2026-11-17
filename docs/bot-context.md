# Bot user information and Workspaces

These tools require a Pontia Session authorized as a Bot. Production access remains closed until Bot Session bindings are connected. Helpers and dreaming cannot use them.

Run commands inside the Bot Session; `PONTIA_SESSION_ID` supplies its identity. Pass values through named arguments. Successful commands print plain text; add `--json` for a JSON result. Failures exit nonzero and print an error to stderr.

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
