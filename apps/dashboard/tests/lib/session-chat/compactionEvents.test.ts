import { expect, test } from "vitest";
import { compactionEventItems } from "../../../src/lib/session-chat/compaction";
import type { EventView, TurnTimelineItem } from "../../../src/api/types";

const event = (phase: string, id: string, time: string): EventView => ({
  event_id: `${id}-${phase}`,
  session_id: "session",
  turn_id: null,
  source: "fixture",
  type: `session.compaction_${phase}`,
  time,
  payload: { operation_id: id },
});
const item = (id: string, time: string): TurnTimelineItem => ({
  item_id: id,
  turn_id: id,
  kind: "assistant",
  role: "assistant",
  raw_kind: "text",
  occurred_at: time,
  title: null,
  status: null,
  content_preview: id,
});

test("persisted completions retain their position and deduplicate live delivery after reload", () => {
  const completion = event("completed", "compact-a", "2026-11-21T10:01:00Z");
  const messages = [item("before", "2026-11-21T10:00:00Z"), item("after", "2026-11-21T10:02:00Z")];
  const events = [
    event("started", "compact-a", "2026-11-21T10:00:10Z"),
    completion,
    event("failed", "compact-b", "2026-11-21T10:03:00Z"),
  ];
  const restored = compactionEventItems(messages, events);
  expect(restored.map((row) => row.item_id)).toEqual([
    "before",
    "compaction-event:compact-a-completed",
    "after",
  ]);
  expect(compactionEventItems(messages, [...events, completion])).toEqual(restored);
  expect(compactionEventItems([], events)).toEqual([restored[1]]);
  expect(messages).toHaveLength(2);
});
