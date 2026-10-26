import { afterEach, expect, test, vi } from "vitest";
import { LiveOutputPublisher } from "../src/live-output.js";
import { MAX_RPC_FRAME_BYTES } from "../src/control-socket.js";

const BUDGET = 1024 * 1024;

afterEach(() => vi.useRealTimers());

function setup() {
  vi.useFakeTimers();
  const bodies: any[] = [];
  const publisher = new LiveOutputPublisher(
    { sessionId: "s", turnId: "t", runtimeId: "r", clientType: "pi" },
    {
      connection: {
        request: async (_method, body: any) => {
          bodies.push(structuredClone(body));
          return {
            accepted: true,
            resync_required: false,
            accepted_sequence: body.sequence ?? body.first_sequence + body.updates.length - 1,
          };
        },
      },
    },
  );
  return { publisher, bodies };
}

test("consecutive reasoning merges and is separated by tool calls and text", async () => {
  const { publisher, bodies } = setup();
  publisher.appendReasoning("Reading");
  publisher.appendReasoning(" files");
  await vi.advanceTimersByTimeAsync(75);
  publisher.appendToolCall({ callId: "one", toolName: "custom", arguments: {} });
  publisher.appendReasoning("Checking");
  publisher.appendText("answer");
  publisher.appendReasoning("Again");
  await vi.advanceTimersByTimeAsync(75);

  expect(bodies[0]).toMatchObject({
    type: "snapshot",
    items: [{ kind: "reasoning", item_id: "reasoning_1", text: "Reading files", truncated: false }],
  });
  expect(bodies[1]).toMatchObject({
    type: "append",
    updates: [
      { type: "tool_call", item_id: "tool_2", call_id: "one" },
      { type: "reasoning_delta", item_id: "reasoning_3", delta: "Checking", truncated: false },
      { type: "assistant_text_delta", item_id: "text_4", delta: "answer" },
      { type: "reasoning_delta", item_id: "reasoning_5", delta: "Again", truncated: false },
    ],
  });
  await publisher.close();
});

test("reasoning over the budget is truncated, later reasoning is dropped and other items continue", async () => {
  const { publisher, bodies } = setup();
  publisher.appendReasoning("r".repeat(BUDGET - 2));
  await vi.advanceTimersByTimeAsync(75);
  // "界" is three UTF-8 bytes and must not be split at the boundary.
  publisher.appendReasoning("ab界cd");
  publisher.appendReasoning("dropped");
  publisher.appendToolCall({ callId: "one", toolName: "custom", arguments: {} });
  publisher.appendReasoning("also dropped");
  publisher.appendText("still visible");
  await vi.advanceTimersByTimeAsync(75);

  expect(bodies[1]).toEqual(
    expect.objectContaining({
      type: "append",
      updates: [
        { type: "reasoning_delta", item_id: "reasoning_1", delta: "ab", truncated: true },
        expect.objectContaining({ type: "tool_call", call_id: "one" }),
        { type: "assistant_text_delta", item_id: "text_3", delta: "still visible" },
      ],
    }),
  );
  await publisher.close();
});

test("reasoning that exactly fills the budget is marked truncated when more arrives", async () => {
  const { publisher, bodies } = setup();
  publisher.appendReasoning("r".repeat(BUDGET));
  await vi.advanceTimersByTimeAsync(75);
  publisher.appendReasoning("more");
  await vi.advanceTimersByTimeAsync(75);

  expect(bodies[0].items[0].truncated).toBe(false);
  expect(bodies[1].type).toBe("snapshot");
  expect(bodies[1].items).toHaveLength(1);
  expect(bodies[1].items[0]).toMatchObject({ kind: "reasoning", truncated: true });
  expect(bodies[1].items[0].text).toHaveLength(BUDGET);
  await publisher.close();
});

test("combined snapshot trims reasoning to fit the RPC frame and keeps assistant text", async () => {
  const { publisher, bodies } = setup();
  publisher.appendReasoning("\u0000".repeat(400_000));
  publisher.appendText("t".repeat(BUDGET));
  await vi.advanceTimersByTimeAsync(75);
  const body = bodies[0];
  expect(
    Buffer.byteLength(
      JSON.stringify({
        jsonrpc: "2.0",
        id: `pi:${Number.MAX_SAFE_INTEGER}`,
        method: "liveOutput.publish",
        params: body,
      }),
    ),
  ).toBeLessThanOrEqual(MAX_RPC_FRAME_BYTES);
  expect(body.items[0]).toMatchObject({ kind: "reasoning", truncated: true });
  expect(body.items[1].text).toHaveLength(BUDGET);
  await publisher.close();
});
