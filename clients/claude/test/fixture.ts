import assert from "node:assert/strict";
import type { SDKMessage, SDKUserMessage } from "@anthropic-ai/claude-agent-sdk";
import type { QueryFactory } from "../src/host.js";

export class Messages implements AsyncIterable<SDKMessage> {
  private pending: SDKMessage[] = [];
  private wake?: () => void;
  private ended = false;
  failure?: Error;
  push(value: unknown) {
    this.pending.push(value as SDKMessage);
    this.wake?.();
  }
  end() {
    this.ended = true;
    this.wake?.();
  }
  async *[Symbol.asyncIterator]() {
    while (!this.ended || this.pending.length) {
      const next = this.pending.shift();
      if (next) yield next;
      else
        await new Promise<void>((resolve) => {
          this.wake = resolve;
        });
    }
    if (this.failure) throw this.failure;
  }
}

export function sdk() {
  const messages = new Messages();
  let prompt: AsyncIterator<SDKUserMessage> | undefined;
  let options: Parameters<QueryFactory>[0]["options"];
  let calls = 0;
  let interrupts = 0;
  let onInterrupt: (() => Promise<void>) | undefined;
  let onClose: (() => void) | undefined;
  const factory: QueryFactory = (args) => {
    calls += 1;
    assert.notEqual(typeof args.prompt, "string");
    prompt = (args.prompt as AsyncIterable<SDKUserMessage>)[Symbol.asyncIterator]();
    options = args.options;
    return {
      [Symbol.asyncIterator]: () => messages[Symbol.asyncIterator](),
      interrupt: async () => {
        interrupts += 1;
        await onInterrupt?.();
      },
      close: () => {
        if (onClose) onClose();
        else messages.end();
      },
    };
  };
  return {
    factory,
    messages,
    get calls() {
      return calls;
    },
    get interrupts() {
      return interrupts;
    },
    get options() {
      return options;
    },
    next: () => prompt!.next(),
    onInterrupt: (fn: () => Promise<void>) => {
      onInterrupt = fn;
    },
    onClose: (fn: () => void) => {
      onClose = fn;
    },
  };
}

export const first = "11111111-1111-4111-8111-111111111111";
export const second = "22222222-2222-4222-8222-222222222222";
export const native = "33333333-3333-4333-8333-333333333333";
export function init(session_id = native) {
  return { type: "system", subtype: "init", session_id };
}
export function assistant(
  input: string | undefined = first,
  content: unknown[] = [{ type: "text", text: "answer" }],
  id = "reply",
  uuid = `envelope-${id}`,
) {
  return {
    type: "assistant",
    uuid,
    session_id: native,
    parent_tool_use_id: null,
    user_message_uuid: input,
    message: { id, role: "assistant", content },
  };
}
export function result(input: string | undefined = first, fields: Record<string, unknown> = {}) {
  return {
    type: "result",
    session_id: native,
    uuid: `result-${input}`,
    user_message_uuid: input,
    subtype: "success",
    is_error: false,
    stop_reason: "end_turn",
    terminal_reason: "completed",
    ...fields,
  };
}
export function stream(event: unknown, input?: string) {
  return {
    type: "stream_event",
    session_id: native,
    parent_tool_use_id: null,
    user_message_uuid: input,
    event,
  };
}
export async function settle() {
  await new Promise<void>((resolve) => setImmediate(resolve));
}
