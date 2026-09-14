import { afterEach, describe, expect, test, vi } from "vitest";
import type { TurnContext } from "../src/context.js";
import { completeToolCallFromMessageUpdate, LiveOutputPublisher } from "../src/live-output.js";

const context: TurnContext & { turnId: string } = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "pi",
  internalEventUrl: "http://localhost/internal/v1/events",
};

function accepted(sequence: number): Response {
  return new Response(JSON.stringify({
    accepted: true,
    duplicate: false,
    resync_required: false,
    accepted_sequence: sequence,
  }), { status: 200 });
}

afterEach(() => {
  vi.useRealTimers();
});

describe("LiveOutputPublisher", () => {
  test("batches text and preserves text-tool-text ordering", async () => {
    vi.useFakeTimers();
    const bodies: any[] = [];
    const fetchImpl = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body));
      bodies.push(body);
      const sequence = body.type === "append"
        ? body.first_sequence + body.updates.length - 1
        : body.sequence;
      return accepted(sequence);
    });
    const publisher = new LiveOutputPublisher(context, {
      fetch: fetchImpl as typeof fetch,
      streamId: "stream_1",
      batchDelayMs: 75,
    });

    publisher.appendText("hello ");
    publisher.appendText("world");
    expect(fetchImpl).not.toHaveBeenCalled();
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

  test("recovers a failed request with the latest complete snapshot", async () => {
    vi.useFakeTimers();
    const bodies: any[] = [];
    const fetchImpl = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body));
      bodies.push(body);
      if (bodies.length === 1) return new Response("offline", { status: 503 });
      const sequence = body.type === "append"
        ? body.first_sequence + body.updates.length - 1
        : body.sequence;
      return accepted(sequence);
    });
    const publisher = new LiveOutputPublisher(context, {
      fetch: fetchImpl as typeof fetch,
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
