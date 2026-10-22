import { afterEach, expect, test, vi } from "vitest";
import { LiveOutputPublisher } from "../src/live-output.js";
import { MAX_RPC_FRAME_BYTES, RpcError } from "../src/control-socket.js";

afterEach(() => vi.useRealTimers());
function setup() {
  vi.useFakeTimers();
  const bodies: any[] = [];
  let disconnected = false;
  const publisher = new LiveOutputPublisher(
    { sessionId: "s", turnId: "t", runtimeId: "r", clientType: "pi" },
    {
      connection: {
        request: async (_method, body: any) => {
          bodies.push(body);
          if (disconnected) throw new RpcError(-32603, "offline");
          return {
            accepted: true,
            resync_required: false,
            accepted_sequence: body.sequence ?? body.first_sequence + body.updates.length - 1,
          };
        },
      },
    },
  );
  return {
    publisher,
    bodies,
    disconnect: () => {
      disconnected = true;
    },
    reconnect: () => {
      disconnected = false;
    },
  };
}
function start(id: string) {
  return { type: "tool_execution_start", toolCallId: id, toolName: "bash", args: { command: id } };
}
function update(id: string, text: string) {
  return {
    type: "tool_execution_update",
    toolCallId: id,
    partialResult: { content: [{ type: "text", text }] },
  };
}
function end(id: string, isError: boolean, text: string) {
  return {
    type: "tool_execution_end",
    toolCallId: id,
    isError,
    result: { content: [{ type: "text", text }] },
  };
}

test("snapshots keep same-name parallel calls, cumulative output and final errors separate", async () => {
  const { publisher, bodies } = setup();
  publisher.appendToolCall({ callId: "one", toolName: "bash", arguments: { command: "one" } });
  await vi.advanceTimersByTimeAsync(75);
  expect(bodies[0].items[0].status).toBeUndefined();
  publisher.toolExecution(start("one"));
  publisher.toolExecution(start("two"));
  publisher.toolExecution(update("one", "a"));
  publisher.toolExecution(update("two", "error"));
  publisher.toolExecution(update("one", "ab"));
  publisher.toolExecution(end("one", false, "ab"));
  publisher.toolExecution(end("two", true, "error"));
  publisher.toolExecution(start("one"));
  await vi.advanceTimersByTimeAsync(75);
  const updates = bodies[1].updates;
  expect(updates.filter((u: any) => u.type === "tool_call").map((u: any) => u.call_id)).toEqual([
    "two",
  ]);
  expect(
    updates
      .filter((u: any) => u.type === "tool_output_updated")
      .map((u: any) => [u.call_id, u.text]),
  ).toEqual([
    ["one", "a"],
    ["two", "error"],
    ["one", "ab"],
  ]);
  expect(
    updates.filter((u: any) => u.type === "tool_status_updated").map((u: any) => u.status),
  ).toEqual([
    { phase: "running" },
    { phase: "running" },
    { phase: "ended", outcome: "succeeded" },
    { phase: "ended", outcome: "failed" },
  ]);
  expect(
    updates.find((u: any) => u.type === "tool_result" && u.call_id === "two").result.error,
  ).toEqual({ content: [{ type: "text", text: "error" }] });
  await publisher.close();
});

test("reconnect snapshots restore terminal facts and replace truncated cumulative output", async () => {
  const { publisher, bodies, disconnect, reconnect } = setup();
  publisher.toolExecution(start("one"));
  await vi.advanceTimersByTimeAsync(75);
  disconnect();
  publisher.toolExecution(update("one", "a"));
  await vi.advanceTimersByTimeAsync(75);
  publisher.toolExecution({
    ...update("one", "ab"),
    partialResult: {
      content: [{ type: "text", text: "ab" }],
      details: { truncation: { truncated: true } },
    },
  });
  publisher.toolExecution(end("one", false, "ab"));
  reconnect();
  await vi.advanceTimersByTimeAsync(500);
  const snapshot = bodies.at(-1);
  expect(snapshot.type).toBe("snapshot");
  expect(snapshot.items).toHaveLength(1);
  expect(snapshot.items[0]).toMatchObject({
    output: { text: "ab", truncated: true },
    status: { phase: "ended", outcome: "succeeded" },
    result: { content: { content: [{ type: "text", text: "ab" }] } },
  });
  await publisher.close();
});

test("JSON escaping cannot disable later results and assistant text", async () => {
  const { publisher, bodies } = setup();
  publisher.toolExecution(start("one"));
  publisher.toolExecution(update("one", "\u0000".repeat(400_000)));
  await vi.advanceTimersByTimeAsync(75);
  expect(bodies[0].items[0].output.truncated).toBe(true);
  expect(Buffer.byteLength(JSON.stringify(bodies[0]))).toBeLessThan(MAX_RPC_FRAME_BYTES);
  publisher.toolExecution(end("one", true, "native failure"));
  publisher.appendText("continued after failure");
  await vi.advanceTimersByTimeAsync(75);
  expect(bodies.at(-1).updates).toContainEqual(
    expect.objectContaining({
      type: "tool_status_updated",
      status: { phase: "ended", outcome: "failed" },
    }),
  );
  expect(bodies.at(-1).updates).toContainEqual(
    expect.objectContaining({ type: "assistant_text_delta", delta: "continued after failure" }),
  );
  await publisher.close();
});

test("combined snapshot accounts for the RPC envelope and tool metadata", async () => {
  const { publisher, bodies } = setup();
  for (let index = 0; index < 20; index++) publisher.toolExecution(start(String(index)));
  publisher.appendText("t".repeat(1024 * 1024));
  publisher.toolExecution(update("0", "x".repeat(1024 * 1024)));
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
  expect(body.items[0].output.truncated).toBe(true);
  expect(body.items.at(-1).text).toHaveLength(1024 * 1024);
  await publisher.close();
});

test("does not invent a call from an uncorrelated completion or infer success from missing isError", async () => {
  const { publisher, bodies } = setup();
  publisher.toolExecution(end("missing", false, "ignored"));
  publisher.appendToolCall({ callId: "one", toolName: "custom", arguments: {} });
  publisher.toolExecution({ type: "tool_execution_end", toolCallId: "one", result: null });
  await vi.advanceTimersByTimeAsync(75);
  expect(bodies[0].items).toHaveLength(1);
  expect(bodies[0].items[0]).toMatchObject({
    status: { phase: "ended" },
    result: { content: null },
  });
  await publisher.close();
});

test("bounds disconnected pending updates and oversized UTF-8 content while retaining execution status", async () => {
  const { publisher, bodies, disconnect, reconnect } = setup();
  publisher.toolExecution(start("one"));
  disconnect();
  await vi.advanceTimersByTimeAsync(75);
  for (let i = 0; i < 300; i++) publisher.toolExecution(update("one", String(i)));
  publisher.toolExecution(update("one", "界".repeat(1024 * 1024)));
  publisher.toolExecution(end("one", true, "failed"));
  reconnect();
  await vi.advanceTimersByTimeAsync(500);
  const snapshot = bodies.at(-1);
  expect(snapshot.type).toBe("snapshot");
  const item = snapshot.items[0];
  expect(Buffer.byteLength(item.output.text)).toBeLessThan(1024 * 1024);
  expect(item.output.truncated).toBe(true);
  expect(item.status).toEqual({ phase: "ended", outcome: "failed" });
  expect(item.result.truncated).toBe(true);
  await publisher.close();
});
