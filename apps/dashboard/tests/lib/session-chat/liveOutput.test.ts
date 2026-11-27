import { expect, test } from "vitest";
import type { TurnView } from "../../../src/api/types";
import type { SessionChatMessage } from "../../../src/lib/session-chat/sessionChat";
import {
  applyLiveOutputEvent,
  markLiveOutputDisconnected,
  mergeLiveOutputMessages,
  removeLiveOutputOverlay,
} from "../../../src/lib/session-chat/liveOutput";

const turn: TurnView = {
  turn_id: "turn-live",
  session_id: "session-1",
  parent_turn_id: null,
  topology_status: "known",
  state: "running",
  input: { summary: "Keep the user prompt" },
  output: null,
  failure: null,
  created_at: "2026-01-01T00:00:00Z",
  started_at: "2026-01-01T00:00:01Z",
  completed_at: null,
  metadata: {},
};

const transcript: SessionChatMessage[] = [
  {
    id: "user",
    turnId: "turn-live",
    role: "user",
    content: "Keep the user prompt",
    status: "sent",
    createdAt: turn.created_at,
  },
  {
    id: "thought",
    turnId: "turn-live",
    role: "assistant",
    content: "",
    status: "pending",
    createdAt: "",
    thoughtSteps: [
      {
        id: "old-tool",
        kind: "tool_call",
        title: "old",
        status: "started",
        content: "old",
        occurredAt: null,
      },
    ],
  },
  {
    id: "assistant",
    turnId: "turn-live",
    role: "assistant",
    content: "transcript partial",
    status: "sent",
    createdAt: "",
  },
];

function snapshot() {
  return {
    type: "snapshot" as const,
    session_id: "session-1",
    turn_id: "turn-live",
    stream_id: "stream-1",
    sequence: 1,
    items: [{ kind: "assistant_text" as const, item_id: "text-1", text: "Hello" }],
  };
}

test("applies token deltas and replaces transcript assistant content without hiding the user", () => {
  let overlays = applyLiveOutputEvent({}, "session-1", snapshot());
  overlays = applyLiveOutputEvent(overlays, "session-1", {
    type: "updates",
    session_id: "session-1",
    turn_id: "turn-live",
    stream_id: "stream-1",
    first_sequence: 2,
    updates: [{ type: "assistant_text_delta", item_id: "text-1", delta: " world" }],
  });

  const messages = mergeLiveOutputMessages(transcript, [turn], overlays);
  expect(messages.map((message) => message.content)).toEqual([
    "Keep the user prompt",
    "Hello world",
  ]);
  expect(messages.some((message) => message.id === "assistant")).toBe(false);
  expect(messages.some((message) => message.thoughtSteps?.[0]?.id === "old-tool")).toBe(false);
});

test("splits streamed output into ordered segments of activity followed by text", () => {
  let overlays = applyLiveOutputEvent({}, "session-1", snapshot());
  overlays = applyLiveOutputEvent(overlays, "session-1", {
    type: "updates",
    session_id: "session-1",
    turn_id: "turn-live",
    stream_id: "stream-1",
    first_sequence: 2,
    updates: [
      {
        type: "tool_call",
        item_id: "tool-1",
        call_id: "call-1",
        tool_name: "read",
        arguments: { path: "README.md" },
        managed_tool_use: { tool_name: "read", input: { type: "read", path: "README.md" } },
      },
      { type: "assistant_text_delta", item_id: "text-2", delta: "Done" },
      {
        type: "tool_call",
        item_id: "tool-2",
        call_id: "call-2",
        tool_name: "bash",
        arguments: { command: "pnpm test" },
        managed_tool_use: { tool_name: "bash", input: { type: "bash", command: "pnpm test" } },
      },
    ],
  });

  const live = mergeLiveOutputMessages(transcript, [turn], overlays).slice(1);
  expect(live.map((message) => [message.content, message.thoughtSteps?.length ?? 0])).toEqual([
    ["Hello", 0],
    ["Done", 1],
    ["", 1],
  ]);
  expect(live.every((message) => message.status === "pending")).toBe(true);
  expect(live[1].thoughtSteps).toMatchObject([
    { title: "Read file", content: "README.md", managedToolUse: { input: { type: "read" } } },
  ]);
  expect(live[2].thoughtSteps).toMatchObject([
    { title: "Run command", content: "pnpm test", managedToolUse: { input: { type: "bash" } } },
  ]);
});

test("consumed user input appears at its position and replaces its transcript copy", () => {
  let overlays = applyLiveOutputEvent({}, "session-1", snapshot());
  overlays = applyLiveOutputEvent(overlays, "session-1", {
    type: "updates",
    session_id: "session-1",
    turn_id: "turn-live",
    stream_id: "stream-1",
    first_sequence: 2,
    updates: [
      { type: "user_input", item_id: "input-1", text: "Change direction" },
      { type: "assistant_text_delta", item_id: "text-2", delta: "Redirected" },
    ],
  });
  const steeredTranscript: SessionChatMessage[] = [
    ...transcript,
    {
      id: "native-immediate",
      turnId: "turn-live",
      role: "user",
      content: "Change direction",
      status: "sent",
      createdAt: "2026-06-11T00:00:02Z",
    },
  ];

  const messages = mergeLiveOutputMessages(steeredTranscript, [turn], overlays);
  expect(messages.map((message) => [message.role, message.content])).toEqual([
    ["user", "Keep the user prompt"],
    ["assistant", "Hello"],
    ["user", "Change direction"],
    ["assistant", "Redirected"],
  ]);
  expect(messages[2]).toMatchObject({ turnId: "turn-live", status: "sent" });
  expect(
    applyLiveOutputEvent(overlays, "session-1", {
      type: "updates",
      session_id: "session-1",
      turn_id: "turn-live",
      stream_id: "stream-1",
      first_sequence: 4,
      updates: [{ type: "user_input", item_id: "input-1", text: "again" }],
    })["turn-live"].awaitingSnapshot,
  ).toBe(true);
});

test("waits for a reconnect snapshot instead of applying deltas twice", () => {
  let overlays = markLiveOutputDisconnected(applyLiveOutputEvent({}, "session-1", snapshot()));
  expect(mergeLiveOutputMessages(transcript, [turn], overlays)).toEqual(transcript);
  overlays = applyLiveOutputEvent(overlays, "session-1", {
    type: "updates",
    session_id: "session-1",
    turn_id: "turn-live",
    stream_id: "stream-1",
    first_sequence: 2,
    updates: [{ type: "assistant_text_delta", item_id: "text-1", delta: " ignored" }],
  });
  expect(overlays["turn-live"].items[0]).toMatchObject({ text: "Hello" });

  overlays = applyLiveOutputEvent(overlays, "session-1", {
    ...snapshot(),
    sequence: 3,
    items: [{ kind: "assistant_text", item_id: "text-1", text: "Recovered" }],
  });
  overlays = applyLiveOutputEvent(overlays, "session-1", {
    type: "updates",
    session_id: "session-1",
    turn_id: "turn-live",
    stream_id: "stream-1",
    first_sequence: 4,
    updates: [{ type: "assistant_text_delta", item_id: "text-1", delta: "!" }],
  });
  expect(overlays["turn-live"].items[0]).toMatchObject({ text: "Recovered!" });
});

test("appends interleaved text deltas to their original items", () => {
  const overlays = applyLiveOutputEvent(
    applyLiveOutputEvent({}, "session-1", snapshot()),
    "session-1",
    {
      type: "updates",
      session_id: "session-1",
      turn_id: "turn-live",
      stream_id: "stream-1",
      first_sequence: 2,
      updates: [
        { type: "assistant_text_delta", item_id: "text-2", delta: "Other" },
        { type: "assistant_text_delta", item_id: "text-1", delta: " world" },
        { type: "assistant_text_delta", item_id: "text-2", delta: " text" },
      ],
    },
  );
  expect(overlays["turn-live"].items).toEqual([
    { kind: "assistant_text", item_id: "text-1", text: "Hello world" },
    { kind: "assistant_text", item_id: "text-2", text: "Other text" },
  ]);
  expect(
    mergeLiveOutputMessages(transcript, [turn], overlays)
      .slice(1)
      .map((message) => message.content),
  ).toEqual(["Hello world", "Other text"]);
});

test("falls back without a snapshot and ignores another Session", () => {
  const unchanged = applyLiveOutputEvent({}, "session-1", {
    ...snapshot(),
    session_id: "session-2",
  });
  expect(mergeLiveOutputMessages(transcript, [turn], unchanged)).toEqual(transcript);
});

test("does not merge an active Turn outside the displayed branch", () => {
  const branchTranscript = transcript.map((message) => ({ ...message, turnId: "turn-current" }));
  const messages = mergeLiveOutputMessages(
    branchTranscript,
    [turn],
    applyLiveOutputEvent({}, "session-1", snapshot()),
    "turn-current",
  );
  expect(messages).toEqual(branchTranscript);
});

test("keeps a closed overlay until timeline convergence removes it", () => {
  let overlays = applyLiveOutputEvent({}, "session-1", snapshot());
  overlays = applyLiveOutputEvent(overlays, "session-1", {
    type: "closed",
    session_id: "session-1",
    turn_id: "turn-live",
    stream_id: "stream-1",
    sequence: 1,
    reason: "producer_closed",
  });
  const closedMessage = mergeLiveOutputMessages(
    transcript,
    [{ ...turn, state: "completed" }],
    overlays,
  ).at(-1);
  expect(closedMessage).toMatchObject({ content: "Hello", status: "pending" });
  expect(removeLiveOutputOverlay(overlays, "turn-live")).toEqual({});

  const expired = applyLiveOutputEvent(
    applyLiveOutputEvent({}, "session-1", snapshot()),
    "session-1",
    {
      type: "closed",
      session_id: "session-1",
      turn_id: "turn-live",
      stream_id: "stream-1",
      sequence: 1,
      reason: "expired",
    },
  );
  expect(mergeLiveOutputMessages(transcript, [turn], expired)).toEqual(transcript);
});
