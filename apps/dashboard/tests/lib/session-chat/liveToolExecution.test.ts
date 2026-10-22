import { expect, test } from "vitest";
import {
  applyLiveOutputEvent,
  markLiveOutputDisconnected,
  mergeLiveOutputMessages,
  type LiveOutputEvent,
  type LiveOutputUpdate,
} from "../../../src/lib/session-chat/liveOutput";
import type { TurnView } from "../../../src/api/types";

const identity = { session_id: "s", turn_id: "t", stream_id: "stream" };
const turn = {
  turn_id: "t",
  session_id: "s",
  state: "running",
  created_at: "now",
  input: { summary: "run" },
} as TurnView;
const snapshot: LiveOutputEvent = {
  type: "snapshot",
  ...identity,
  sequence: 1,
  items: [
    {
      kind: "tool_call",
      item_id: "a",
      call_id: "a",
      tool_name: "bash",
      arguments: { command: "first" },
    },
    {
      kind: "tool_call",
      item_id: "b",
      call_id: "b",
      tool_name: "bash",
      arguments: { command: "second" },
    },
  ],
};
function updates(first_sequence: number, updates: LiveOutputUpdate[]): LiveOutputEvent {
  return { type: "updates", ...identity, first_sequence, updates };
}

test("parallel calls retain output, result and execution status independently", () => {
  const overlay = applyLiveOutputEvent(
    applyLiveOutputEvent({}, "s", snapshot),
    "s",
    updates(2, [
      { type: "tool_output_delta", call_id: "a", delta: "a", truncated: false },
      { type: "tool_output_updated", call_id: "b", text: "x", truncated: false },
      { type: "tool_output_delta", call_id: "a", delta: "b", truncated: false },
      { type: "tool_output_updated", call_id: "b", text: "xy", truncated: true },
      { type: "tool_result", call_id: "a", result: { content: "ab", truncated: false } },
      {
        type: "tool_status_updated",
        call_id: "a",
        status: { phase: "ended", outcome: "succeeded" },
      },
      { type: "tool_result", call_id: "b", result: { error: "failed", truncated: false } },
      { type: "tool_status_updated", call_id: "b", status: { phase: "ended", outcome: "failed" } },
    ]),
  );
  const messages = mergeLiveOutputMessages([], [turn], overlay, "t");
  expect(messages.at(-1)?.content).toBe("");
  expect(messages.at(-1)?.thoughtSteps).toMatchObject([
    { callId: "a", status: "succeeded", processOutput: { text: "ab" }, result: { content: "ab" } },
    {
      callId: "b",
      status: "failed",
      processOutput: { text: "xy", truncated: true },
      result: { content: "failed", status: "error" },
    },
  ]);
  const restored = applyLiveOutputEvent(markLiveOutputDisconnected(overlay), "s", {
    type: "snapshot",
    ...identity,
    sequence: overlay.t.sequence,
    items: overlay.t.items,
  });
  expect(mergeLiveOutputMessages([], [turn], restored, "t")).toEqual(messages);
});

test("generated and ended-unknown stay distinct and closure does not imply success", () => {
  let overlay = applyLiveOutputEvent({}, "s", snapshot);
  overlay = applyLiveOutputEvent(
    overlay,
    "s",
    updates(2, [{ type: "tool_status_updated", call_id: "a", status: { phase: "ended" } }]),
  );
  overlay = applyLiveOutputEvent(overlay, "s", {
    type: "closed",
    ...identity,
    sequence: 3,
    reason: "producer_closed",
  });
  expect(
    mergeLiveOutputMessages([], [turn], overlay, "t")
      .at(-1)
      ?.thoughtSteps?.map((step) => step.status),
  ).toEqual(["ended", "generated"]);
});

test("gaps and duplicate batches cannot append tool output again; stale snapshots cannot regress it", () => {
  let overlay = applyLiveOutputEvent({}, "s", snapshot);
  const event = updates(2, [
    { type: "tool_output_delta", call_id: "a", delta: "once", truncated: false },
  ]);
  overlay = applyLiveOutputEvent(overlay, "s", event);
  const before = overlay.t.items;
  overlay = applyLiveOutputEvent(overlay, "s", event);
  expect(overlay.t.awaitingSnapshot).toBe(true);
  expect(overlay.t.items).toEqual(before);
  overlay = applyLiveOutputEvent(overlay, "s", snapshot);
  expect(overlay.t.items).toEqual(before);
  overlay = applyLiveOutputEvent(overlay, "s", {
    type: "snapshot",
    ...identity,
    sequence: 2,
    items: before,
  });
  expect(overlay.t.awaitingSnapshot).toBe(false);
  overlay = applyLiveOutputEvent(
    overlay,
    "s",
    updates(4, [{ type: "tool_output_delta", call_id: "a", delta: "lost", truncated: false }]),
  );
  expect(overlay.t.items).toEqual(before);
  expect(overlay.t.awaitingSnapshot).toBe(true);
});

test("tool-first output keeps text and call identity across metadata replacement and reconnect", () => {
  let overlay = applyLiveOutputEvent({}, "s", snapshot);
  overlay = applyLiveOutputEvent(
    overlay,
    "s",
    updates(2, [
      { type: "assistant_text_delta", item_id: "response", delta: "body" },
      { type: "tool_output_updated", call_id: "a", text: "a", truncated: false },
      { type: "tool_output_updated", call_id: "a", text: "ab", truncated: false },
      {
        type: "tool_status_updated",
        call_id: "a",
        status: { phase: "ended", outcome: "succeeded" },
      },
      { type: "tool_result", call_id: "a", result: { content: "ab", truncated: false } },
    ]),
  );
  overlay = applyLiveOutputEvent(overlay, "s", {
    type: "snapshot",
    ...identity,
    sequence: 7,
    items: overlay.t.items.map((item) =>
      item.kind === "tool_call" && item.call_id === "a"
        ? { ...item, tool_name: "Updated invocation", arguments: { command: "changed" } }
        : item,
    ),
  });
  const messages = mergeLiveOutputMessages([], [turn], overlay, "t");
  expect(messages.at(-1)?.content).toBe("body");
  expect(messages.at(-1)?.thoughtSteps).toMatchObject([
    { callId: "a", status: "succeeded", processOutput: { text: "ab" }, result: { content: "ab" } },
    { callId: "b", status: "generated" },
  ]);
  overlay = applyLiveOutputEvent(markLiveOutputDisconnected(overlay), "s", {
    type: "snapshot",
    ...identity,
    sequence: 7,
    items: overlay.t.items,
  });
  expect(mergeLiveOutputMessages([], [turn], overlay, "t")).toEqual(messages);
  overlay = applyLiveOutputEvent(
    overlay,
    "s",
    updates(8, [
      { type: "tool_output_updated", call_id: "a", text: "", truncated: false },
      { type: "tool_status_updated", call_id: "a", status: { phase: "running" } },
      { type: "assistant_text_delta", item_id: "response", delta: " after" },
    ]),
  );
  expect(mergeLiveOutputMessages([], [turn], overlay, "t").at(-1)).toMatchObject({
    content: "body after",
    thoughtSteps: [
      { callId: "a", status: "succeeded", processOutput: { text: "" }, result: { content: "ab" } },
      { callId: "b", status: "generated" },
    ],
  });
});

test("unknown calls discard the entire update batch and wait for a snapshot", () => {
  const initial = applyLiveOutputEvent({}, "s", snapshot);
  const overlay = applyLiveOutputEvent(
    initial,
    "s",
    updates(2, [
      { type: "tool_output_delta", call_id: "a", delta: "not committed", truncated: false },
      { type: "tool_result", call_id: "missing", result: { content: "orphan", truncated: false } },
    ]),
  );
  expect(overlay.t.items).toEqual(initial.t.items);
  expect(overlay.t.sequence).toBe(1);
  expect(overlay.t.awaitingSnapshot).toBe(true);
});
