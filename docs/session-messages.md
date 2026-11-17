# Read Session messages

Read recent messages or a fragment starting at a Turn without sending input or changing the Session's execution state. Query execution status separately.

Send an authenticated `POST /api/v1/sessions/messages/read` with this JSON body:

```json
{"session_id":"<target-session-id>","limit":10}
```

The response's `data` contains chronological `items`, `session_id`, `has_more`, and `next`. Each item retains its `turn_id` and opaque client `item_id`. Keep both with a citation; use the Turn ID to reread its surrounding fragment. Native history must be available; an unavailable source returns an error instead of an empty conversation.

Optional query fields:

| Field | Meaning |
| --- | --- |
| `turn_id` | Inclusive starting Turn; omit for the newest page when reading backward. |
| `direction` | `backward` (default) or `forward`. Forward without an anchor starts at the oldest Turn. |
| `limit` | Turns scanned per page, 1–100, default 10. |
| `categories` | Any nonempty selection of `user`, `assistant`, `other`, `tool_call`, `tool_result`. Defaults to the first three. |

To inspect tools around a known Turn:

```json
{"session_id":"<target-session-id>","turn_id":"<turn-id>","limit":1,"categories":["tool_call","tool_result"]}
```

Tool details retain the client's timeline previews. For subsequent pages, submit the entire `next` object unchanged. `has_more` means more Turns remain to scan, so an empty filtered page can still have a next page.

## Bot tool

Authorized Bot, retrieval helper, and dreaming Sessions use named CLI arguments:

```bash
pontia bot session-messages --session-id '<target-session-id>' --limit 10
pontia bot session-messages --session-id '<target-session-id>' \
  --turn-id '<turn-id>' --limit 1 --category tool_call --category tool_result
```

`PONTIA_SESSION_ID` identifies the caller; `--session-id` selects the target. Results are plain text by default, retaining message content, Turn IDs and item IDs. When another page exists, the output includes a continuation command that preserves the direction, categories, page size and history source. Follow it even when the current page contains no matching messages.

Add `--json` to get the same result fields as the API. To continue from its `next` object, pass `session_id` as `--session-id`, `turn_id` as `--turn-id`, `direction` as `--direction`, `limit` as `--limit`, each category as a repeated `--category`, and `history_source` as `--history-source` when present. The source is `native` or `file`; retain the returned value on subsequent pages.

Failures exit nonzero and print an error to stderr. Bot tool access remains closed until Bot Session bindings are connected; the authenticated Session query can be used independently.
