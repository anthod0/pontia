# Bot memory tools

Run memory commands from a Bot Session. Bots on the same device share their memories.

Pass values through named arguments. Repeat `--keyword` or `--id` for lists. Results are plain text by default; add `--json` for a JSON result. Failures exit nonzero and print an error to stderr.

```bash
pontia bot memory create --kind fact --content 'The project uses Rust.' \
  --keyword project --keyword Rust --sources 'The user chose Rust during project setup.'

pontia bot memory search --keyword project --keyword Rust --limit 10
pontia bot memory search --keyword Rust --json
```

Use shell quoting for text containing spaces or punctuation. Multiline values can be passed directly as quoted arguments. Use `--help` on an action to see its arguments.

## Keywords and scope

Supply 1–32 keywords, each containing 1–128 characters after normalization. Leading and trailing whitespace is removed, internal whitespace is collapsed to one space, letters are lowercased and duplicate keywords are removed. Blank keywords and non-whitespace control characters are rejected.

Each keyword matches as a whole. `中`, `中文`, `Rust项目`, `C++` and `agent client` are distinct keywords; querying `agent` does not match `agent client`. Include both a compound term and its useful component terms when both should be searchable. Punctuation is literal. Queries match any supplied keyword. Words found only in the body cannot retrieve a memory.

Without `--workspace-id`, reads cover all active memories on the device. An explicit `--workspace-id` selects exactly that Workspace, excluding device-wide memories. A Session assigned to a specific Workspace remains restricted to it, including reads by ID and graph traversal.

Search returns complete memories, relevance, score and whether more candidates remain. It defaults to 10 results and accepts limits from 1 to 30. `has_more` in JSON output describes unused results in the 100-candidate search set; it is not a pagination cursor. Scores are comparable within one query.

## Commands

| Action | Arguments | JSON result fields (`--json`) |
| --- | --- | --- |
| `create` | Required: `--kind`, `--content`, repeated `--keyword`. Optional: `--workspace-id`, `--session-id`, `--importance`, `--occurred-at`, `--sources`. | `memory` |
| `read` | Repeated `--id` (1–100), optional `--workspace-id`. | `memories` |
| `search` | Repeated `--keyword`, optional `--workspace-id`, `--limit`. | `results`, `has_more` |
| `neighbors` | `--id`, optional `--workspace-id`, `--limit` (1–100, default 30), `--after`. | `memories`, `next_cursor` |
| `use` | Repeated `--id` (1–100), optional `--workspace-id`. | `affected` |

Types are `fact`, `decision`, `event`, `lesson` and `observation`. Importance is `low`, `normal` or `high`, defaulting to `normal` on creation. Content is preserved verbatim, must be nonblank and is limited to 65,536 UTF-8 bytes. Sources are plain text limited to 16,384 UTF-8 bytes. Neither accepts NUL. `--occurred-at` accepts an RFC 3339 timestamp. IDs are opaque nonblank strings of at most 256 bytes without control characters. `--session-id` on creation associates a source Session with the memory; it does not change the caller's identity.

Archived memories are excluded from searches, ID reads and both ends of graph traversal.

ID reads omit missing, archived and out-of-scope records. `use` reports how many active records in scope were updated. It refreshes `last_read_at` only; searches and other reads never refresh it. Report only memories actually used in a response or decision.

For neighbors, plain-text output includes a command for the next page. In JSON mode, pass `next_cursor` as `--after`, keeping `--id`, `--workspace-id` and `--limit` unchanged. A null cursor means no further neighbors. Traversal returns one hop; the agent chooses whether to continue from another memory.
