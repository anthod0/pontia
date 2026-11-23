import { expect, test } from "vitest";
import {
  compactionCompletedLabel,
  compactionEventItems,
  compactionStatus,
} from "../../../src/lib/session-chat/compaction";
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

test("completion checkpoints carry the reported token counts", () => {
  const completion = {
    ...event("completed", "compact-a", "2026-11-21T10:01:00Z"),
    payload: { operation_id: "compact-a", tokens_before: 152_340, tokens_after: null },
  };
  expect(compactionEventItems([], [completion])[0].compaction_tokens).toEqual({
    before: 152_340,
    after: null,
  });
});

test.each([
  [{ before: 152_340, after: 18_200 }, "Context compacted · 152k → 18k"],
  [{ before: 1_250_000, after: 640 }, "Context compacted · 1.3m → 640"],
  [{ before: 152_340, after: null }, "Context compacted · 152k before"],
  [{ before: null, after: 18_200 }, "Context compacted · 18k after"],
  [{ before: null, after: null }, "Context compacted"],
  [undefined, "Context compacted"],
])("completion label shows the token counts that are present: %j", (tokens, label) => {
  expect(compactionCompletedLabel(tokens)).toBe(label);
});

test("the completion status uses the same token label", () => {
  const completion = {
    ...event("completed", "compact-a", "2026-11-21T10:01:00Z"),
    payload: { operation_id: "compact-a", tokens_before: 152_340, tokens_after: 18_200 },
  };
  expect(compactionStatus([completion], null)?.message).toBe("Context compacted · 152k → 18k");
});
