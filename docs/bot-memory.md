# Bot memory tools

Memory access is currently closed: all Sessions receive `forbidden` until Bot Session identity binding is available. The commands and input formats below describe the memory interface.

Memory tools require a running `pontiad`, a local API token and a Session authorized by the Bot application. The CLI reads `PONTIA_SESSION_ID` automatically. A caller cannot choose its role through command arguments.

```bash
pontia bot memory create --input memory.json
pontia bot memory search --input query.json
```

Omit `--input` or use `--input -` to read a JSON object from stdin. Successful commands print JSON; errors exit nonzero.

Example `memory.json`:

```json
{
  "kind": "fact",
  "content": "The project uses Rust.",
  "keywords": ["project", "Rust"],
  "importance": "normal",
  "sources": "The user chose Rust during project setup."
}
```

Example `query.json`:

```json
{"keywords": ["project", "Rust"], "limit": 10}
```

## Keywords and scope

Supply 1–32 keywords, each containing 1–128 characters after normalization. Leading and trailing whitespace is removed, internal whitespace is collapsed to one space, letters are lowercased and duplicate keywords are removed. Blank keywords and non-whitespace control characters are rejected.

Each keyword matches as a whole. `中`, `中文`, `Rust项目`, `C++` and `agent client` are distinct keywords; querying `agent` does not match `agent client`. Include both a compound term and its useful component terms when both should be searchable. Punctuation is literal. Queries match any supplied keyword. Words found only in the body cannot retrieve a memory.

Without `workspace_id`, reads cover all active memories on the device. An explicit `workspace_id` selects exactly that Workspace, excluding device-wide memories. A Session assigned to a specific Workspace remains restricted to it, including reads by ID and graph traversal.

Search returns `results`, each containing the complete `memory`, `relevance` and `score`, plus `has_more`. Search defaults to 10 results and accepts limits from 1 to 30. `has_more` describes unused results in the 100-candidate search set; it is not a pagination cursor. Scores are comparable within one query.

## Commands

| Action | JSON input | Result |
| --- | --- | --- |
| `create` | Required: `kind`, `content`, `keywords`. Optional: `workspace_id`, `session_id`, `importance`, `occurred_at`, `sources`. | `memory` |
| `update` | Required: `id`, `kind`, `content`, `keywords`, `importance`. Optional: `occurred_at`, `sources`. Replaces these fields; omitted optional fields become null. | `affected` |
| `archive` | `id`, `archived` (`true` or `false`) | `affected` |
| `link` / `unlink` | `memory_a_id`, `memory_b_id` | `affected` |
| `read` | `ids` (1–100), optional `workspace_id` | `memories` |
| `search` | `keywords`, optional `workspace_id`, `limit` | `results`, `has_more` |
| `neighbors` | `id`, optional `workspace_id`, `limit` (1–100, default 30), `after` | `memories`, `next_cursor` |
| `use` | `ids` (1–100), optional `workspace_id` | `affected` |

Types are `fact`, `decision`, `event`, `lesson` and `observation`. Importance is `low`, `normal` or `high`, defaulting to `normal` on creation. Content is preserved verbatim, must be nonblank and is limited to 65,536 UTF-8 bytes. Sources are plain text limited to 16,384 UTF-8 bytes. Neither accepts NUL. `occurred_at` accepts an RFC 3339 timestamp. IDs are opaque nonblank strings of at most 256 bytes without control characters.

Updates preserve the memory's Workspace, associated Session, author and creation time. Archiving preserves the memory and its links. Archived memories are excluded from searches, ID reads and both ends of graph traversal; use `archive` with `false` to restore a known ID. Links are undirected, repeated links have no additional effect and a memory cannot link to itself. Link changes require both endpoints to be active and in scope.

ID reads omit missing, archived and out-of-scope records. `use` reports how many active records in scope were updated. It refreshes `last_read_at` only; searches and other reads never refresh it. Report only memories actually used in a response or decision.

For neighbors, pass `next_cursor` as the next request's `after` value, keeping the origin and Workspace unchanged. A null cursor means no further neighbors. Traversal returns one hop; the agent chooses whether to continue from another memory.

| Role | Allowed operations |
| --- | --- |
| Bot | Create, search, read, neighbors, use |
| Retrieval helper | Search, read, neighbors |
| Dreaming | Create, update, archive, link, unlink, search, read, neighbors |

Unknown or unbound Sessions are denied. Invalid input, denied operations and unavailable write targets produce distinct errors; CLI errors include the service's error code.
