import { expect, test } from "vitest";
import type { TurnView } from "../../../src/api/types";
import {
  applyLiveOutputEvent,
  mergeLiveOutputMessages,
  type LiveOutputEvent,
  type LiveOutputUpdate,
} from "../../../src/lib/session-chat/liveOutput";

const identity = { session_id: "s", turn_id: "t", stream_id: "stream" };
const turn = {
  turn_id: "t",
  session_id: "s",
  state: "running",
  created_at: "now",
  input: { summary: "run" },
} as TurnView;

function updates(first_sequence: number, updates: LiveOutputUpdate[]): LiveOutputEvent {
  return { type: "updates", ...identity, first_sequence, updates };
}

function start(): LiveOutputEvent {
  return {
    type: "snapshot",
    ...identity,
    sequence: 1,
    items: [{ kind: "reasoning", item_id: "r1", text: "Reading" }],
  };
}

test("reasoning deltas render as thinking steps interleaved with tool calls in item order", () => {
  const overlays = applyLiveOutputEvent(
    applyLiveOutputEvent({}, "s", start()),
    "s",
    updates(2, [
      { type: "reasoning_delta", item_id: "r1", delta: " files" },
      { type: "tool_call", item_id: "c1", call_id: "c1", tool_name: "bash", arguments: {} },
      { type: "reasoning_delta", item_id: "r2", delta: "Checking" },
      { type: "assistant_text_delta", item_id: "a1", delta: "answer" },
    ]),
  );
  const message = mergeLiveOutputMessages([], [turn], overlays, "t").at(-1);
  expect(message?.content).toBe("answer");
  expect(message?.thoughtSteps?.map((step) => [step.kind, step.content])).toEqual([
    ["thinking", "Reading files"],
    ["tool_call", "{}"],
    ["thinking", "Checking"],
  ]);
});

test("a reasoning delta for an item of another kind waits for a snapshot", () => {
  const overlays = applyLiveOutputEvent(
    applyLiveOutputEvent({}, "s", {
      ...start(),
      items: [{ kind: "assistant_text", item_id: "a1", text: "answer" }],
    }),
    "s",
    updates(2, [{ type: "reasoning_delta", item_id: "a1", delta: "x" }]),
  );
  expect(overlays.t.awaitingSnapshot).toBe(true);
  expect(overlays.t.items).toEqual([{ kind: "assistant_text", item_id: "a1", text: "answer" }]);
});

test("truncated reasoning stays marked after later deltas", () => {
  const overlays = applyLiveOutputEvent(
    applyLiveOutputEvent({}, "s", start()),
    "s",
    updates(2, [
      { type: "reasoning_delta", item_id: "r1", delta: " partial", truncated: true },
      { type: "reasoning_delta", item_id: "r1", delta: " more" },
    ]),
  );
  const steps = mergeLiveOutputMessages([], [turn], overlays, "t").at(-1)?.thoughtSteps;
  expect(steps).toMatchObject([
    { kind: "thinking", content: "Reading partial more", truncated: true },
  ]);
});
