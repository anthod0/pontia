import { afterEach, describe, expect, test, vi } from "vitest";
import { RpcError } from "../src/control-socket.js";
import type { TurnContext } from "../src/context.js";
import { completeToolCallFromMessageUpdate, LiveOutputPublisher } from "../src/live-output.js";

const context: TurnContext & { turnId: string } = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "pi",
};

function accepted(sequence: number) {
  return {
    accepted: true,
    duplicate: false,
    resync_required: false,
    accepted_sequence: sequence,
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("LiveOutputPublisher", () => {
  test("batches text and preserves text-tool-text ordering", async () => {
    vi.useFakeTimers();
    const bodies: any[] = [];
    const request = vi.fn(async (method: string, body: any) => {
      expect(method).toBe("liveOutput.publish");
      bodies.push(body);
      const sequence = body.type === "append"
        ? body.first_sequence + body.updates.length - 1
        : body.sequence;
      return accepted(sequence);
    });
    const publisher = new LiveOutputPublisher(context, {
      connection: { request },
      streamId: "stream_1",
      batchDelayMs: 75,
    });

    publisher.appendText("hello ");
    publisher.appendText("world");
    expect(request).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(75);

    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({
      type: "snapshot",
      sequence: 2,
      items: [{ kind: "assistant_text", item_id: "text_1", text: "hello world" }],
    });

    publisher.appendToolCall({
      callId: "call_1",
      toolName: "read",
      arguments: { path: "README.md" },
    });
    publisher.appendText("done");
    await vi.advanceTimersByTimeAsync(75);

    expect(bodies[1]).toMatchObject({
      type: "append",
      first_sequence: 3,
      updates: [
        {
          type: "tool_call",
          item_id: "tool_2",
          call_id: "call_1",
          tool_name: "read",
          arguments: { path: "README.md" },
        },
        { type: "assistant_text_delta", item_id: "text_3", delta: "done" },
      ],
    });
  });

  test("maps supported Pi tools before reporting live output", async () => {
    vi.useFakeTimers();
    const bodies: any[] = [];
    const request = vi.fn(async (method: string, body: any) => {
      expect(method).toBe("liveOutput.publish");
      bodies.push(body);
      return accepted(body.sequence);
    });
    const publisher = new LiveOutputPublisher(context, {
      connection: { request },
      streamId: "stream_1",
      batchDelayMs: 75,
    });

    publisher.appendToolCall({ callId: "read_1", toolName: "read", arguments: { path: "src/app.ts", start_line: 4 } });
    publisher.appendToolCall({ callId: "edit_1", toolName: "edit", arguments: { path: "src/app.ts", edits: [{ oldText: "a", newText: "b" }] } });
    publisher.appendToolCall({ callId: "write_1", toolName: "write", arguments: { path: "out.txt", content: "done" } });
    publisher.appendToolCall({ callId: "bash_1", toolName: "bash", arguments: { command: "pnpm test", timeout: 30 } });
    publisher.appendToolCall({ callId: "custom_1", toolName: "custom", arguments: { value: true } });
    publisher.appendToolCall({ callId: "edit_2", toolName: "edit", arguments: { path: "broken.ts" } });
    await vi.advanceTimersByTimeAsync(75);

    expect(bodies[0].items.map((item: any) => item.managed_tool_use)).toEqual([
      { tool_name: "read", input: { type: "read", path: "src/app.ts", start_line: 4 } },
      { tool_name: "edit", input: { type: "edit", path: "src/app.ts", edits_count: 1 } },
      { tool_name: "write", input: { type: "write", path: "out.txt" } },
      { tool_name: "bash", input: { type: "bash", command: "pnpm test", timeout: 30 } },
      undefined,
      undefined,
    ]);
  });

  test("recovers a failed request with the latest complete snapshot", async () => {
    vi.useFakeTimers();
    const bodies: any[] = [];
    const request = vi.fn(async (method: string, body: any) => {
      expect(method).toBe("liveOutput.publish");
      bodies.push(body);
      if (bodies.length === 1) throw new RpcError(-32603, "temporarily unavailable");
      const sequence = body.type === "append"
        ? body.first_sequence + body.updates.length - 1
        : body.sequence;
      return accepted(sequence);
    });
    const publisher = new LiveOutputPublisher(context, {
      connection: { request },
      streamId: "stream_1",
      batchDelayMs: 75,
    });

    publisher.appendText("hello");
    await vi.advanceTimersByTimeAsync(75);
    publisher.appendText(" world");
    await vi.advanceTimersByTimeAsync(500);

    expect(bodies).toHaveLength(2);
    expect(bodies[1]).toMatchObject({
      type: "snapshot",
      sequence: 2,
      items: [{ kind: "assistant_text", text: "hello world" }],
    });

    publisher.appendText("!");
    await vi.advanceTimersByTimeAsync(75);
    expect(bodies[2]).toMatchObject({
      type: "append",
      first_sequence: 3,
      updates: [{ type: "assistant_text_delta", delta: "!" }],
    });
  });

  test("resynchronizes gaps with a snapshot and flushes before closing", async () => {
    vi.useFakeTimers();
    const bodies: any[] = [];
    const request = vi.fn(async (_method: string, body: any) => {
      bodies.push(body);
      if (bodies.length === 2) return { accepted: false, accepted_sequence: 0, resync_required: true };
      return accepted(body.sequence);
    });
    const publisher = new LiveOutputPublisher(context, { connection: { request } });
    publisher.appendText("hello");
    await vi.advanceTimersByTimeAsync(75);
    publisher.appendText(" world");
    await vi.advanceTimersByTimeAsync(75);
    await publisher.close();

    expect(bodies.map((body) => body.type)).toEqual(["snapshot", "append", "snapshot", "stream_closed"]);
    expect(bodies[2]).toMatchObject({ sequence: 2, items: [{ text: "hello world" }] });
    expect(bodies[3]).toMatchObject({ sequence: 3 });
    await vi.advanceTimersByTimeAsync(1_000);
    expect(bodies).toHaveLength(4);
  });

  test.each([-32601, -32602, -32004, -32009])("stops publishing after permanent RPC rejection %s", async (code) => {
    vi.useFakeTimers();
    const request = vi.fn(async () => { throw new RpcError(code, "rejected"); });
    const publisher = new LiveOutputPublisher(context, { connection: { request } });
    publisher.appendText("hello");
    await vi.advanceTimersByTimeAsync(75);
    publisher.appendText(" world");
    await vi.advanceTimersByTimeAsync(1_000);
    await publisher.close();
    expect(request).toHaveBeenCalledTimes(1);
  });

  test("keeps a snapshot consistent while its request waits for reconnect", async () => {
    vi.useFakeTimers();
    let acknowledge: (() => void) | undefined;
    const bodies: any[] = [];
    const request = vi.fn(async (_method: string, body: any) => {
      if (bodies.length === 0) await new Promise<void>((resolve) => { acknowledge = resolve; });
      bodies.push(body);
      return accepted(body.sequence ?? body.first_sequence + body.updates.length - 1);
    });
    const publisher = new LiveOutputPublisher(context, { connection: { request } });
    publisher.appendText("hello");
    await vi.advanceTimersByTimeAsync(75);
    publisher.appendText(" world");
    acknowledge!();
    await vi.advanceTimersByTimeAsync(75);
    await publisher.close();
    expect(bodies[0]).toMatchObject({ sequence: 1, items: [{ text: "hello" }] });
    expect(bodies[1]).toMatchObject({ first_sequence: 2, updates: [{ delta: " world" }] });
  });

  test("extracts only complete Pi tool calls", () => {
    expect(completeToolCallFromMessageUpdate({
      assistantMessageEvent: {
        type: "toolcall_delta",
        delta: "{\"path\":",
      },
    })).toBeUndefined();
    expect(completeToolCallFromMessageUpdate({
      assistantMessageEvent: {
        type: "toolcall_end",
        toolCall: {
          id: "call_1",
          name: "read",
          arguments: { path: "README.md" },
        },
      },
    })).toEqual({
      callId: "call_1",
      toolName: "read",
      arguments: { path: "README.md" },
    });
  });
});
