# Bot user information and Workspaces

These tools require a Pontia Session authorized as a Bot. Production access remains closed until Bot Session bindings are connected. Helpers and dreaming cannot use them.

Run commands inside the Bot Session; `PONTIA_SESSION_ID` supplies its identity. Pass a JSON file with `--input <path>`, or pipe JSON through stdin. Successful commands print one JSON result; failures exit nonzero.

## Update user information

```bash
printf '%s' '{"old_text":"语言：中文","new_text":"语言：英文"}' | pontia bot update-user
```

User information is shared by all Bots on the device in `$PONTIA_HOME/bot/USER.md` (default `~/.pontia/bot/USER.md`). Keep explicitly stated, lasting preferences and identity information here.

- `old_text` must match exactly once. Matching is case-sensitive and preserves whitespace.
- Empty `old_text` appends `new_text`; empty `new_text` deletes the match.
- A missing file starts empty. The result may contain at most 2,000 Unicode characters.
- Missing or multiple matches, including overlapping matches, and overlong results leave the file unchanged. The error includes the current text so the Bot can retry.
- Success confirms the update without returning the text. Each update uses the latest contents, preserving other Bots' changes to different text.

## Register a Workspace

```bash
printf '%s' '{"root_id":"projects","path":"my-app","description":"用户的网站项目"}' | pontia bot workspace register
```

`root_id` selects a configured Workspace root. `path` is relative to that root; the directory must already exist within it. Optional `name` sets the display name. Registering the same canonical directory reuses its Workspace ID and activates it.

Optional `description` sets the Workspace description. Omitting it preserves any existing description. Descriptions may contain at most 2,000 Unicode characters and appear in the existing Workspace queries.

## Change a Workspace description

```bash
printf '%s' '{"workspace_id":"<workspace-id>","description":"项目用途与背景"}' | pontia bot workspace describe
```

Use `null` to clear the description. Changing it preserves the Workspace ID and activation state.
