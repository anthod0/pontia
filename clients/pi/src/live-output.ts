import { randomUUID } from "node:crypto";
import type { TurnContext } from "./context.js";
import { asRecord, optionalString, parseJsonResponse } from "./internal-api.js";

const DEFAULT_BATCH_DELAY_MS = 75;
const RETRY_DELAY_MS = 500;

export type ManagedToolUse = {
  tool_name: string;
  input:
    | { type: "read"; path: string; start_line?: number; end_line?: number }
    | { type: "edit"; path: string; edits_count: number }
    | { type: "write"; path: string }
    | { type: "bash"; command: string; timeout?: number };
};

export type LiveOutputItem =
  | { kind: "assistant_text"; item_id: string; text: string }
  | { kind: "tool_call"; item_id: string; call_id: string; tool_name: string; arguments: unknown; managed_tool_use?: ManagedToolUse };

export type LiveOutputUpdate =
  | { type: "assistant_text_delta"; item_id: string; delta: string }
  | { type: "tool_call"; item_id: string; call_id: string; tool_name: string; arguments: unknown; managed_tool_use?: ManagedToolUse };

export interface CompleteToolCall {
  callId: string;
  toolName: string;
  arguments: unknown;
}

export interface LiveOutputPublisherLike {
  appendText(delta: string): void;
  appendToolCall(toolCall: CompleteToolCall): void;
  close(): Promise<void>;
}

interface PendingUpdate {
  sequence: number;
  update: LiveOutputUpdate;
}

interface LiveOutputResponse {
  accepted: boolean;
  accepted_sequence: number;
  resync_required: boolean;
}

export interface LiveOutputPublisherOptions {
  fetch?: typeof fetch;
  streamId?: string;
  batchDelayMs?: number;
}

export class LiveOutputPublisher implements LiveOutputPublisherLike {
  private readonly fetchImpl: typeof fetch;
  private readonly url: string;
  private readonly context: TurnContext & { turnId: string };
  private readonly streamId: string;
  private readonly batchDelayMs: number;
  private readonly items: LiveOutputItem[] = [];
  private readonly pending: PendingUpdate[] = [];
  private sequence = 0;
  private itemSequence = 0;
  private needsSnapshot = true;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private flushing: Promise<boolean> | undefined;
  private closing = false;
  private disabled = false;

  constructor(context: TurnContext & { turnId: string }, options: LiveOutputPublisherOptions = {}) {
    this.context = context;
    this.fetchImpl = options.fetch ?? fetch;
    this.streamId = options.streamId ?? `stream_${randomUUID()}`;
    this.batchDelayMs = options.batchDelayMs ?? DEFAULT_BATCH_DELAY_MS;
    const url = new URL(context.internalEventUrl);
    url.pathname = url.pathname.replace(/\/events\/?$/, "/live-output");
    this.url = url.toString();
  }

  appendText(delta: string): void {
    if (this.closing || this.disabled || delta.length === 0) return;
    let item = this.items.at(-1);
    if (item?.kind !== "assistant_text") {
      item = { kind: "assistant_text", item_id: this.nextItemId("text"), text: "" };
      this.items.push(item);
    }
    item.text += delta;
    this.enqueue({ type: "assistant_text_delta", item_id: item.item_id, delta });
  }

  appendToolCall(toolCall: CompleteToolCall): void {
    if (
      this.closing ||
      this.disabled ||
      this.items.some((item) => item.kind === "tool_call" && item.call_id === toolCall.callId)
    ) return;
    const managedToolUse = managedToolUseFor(toolCall);
    const item: LiveOutputItem = {
      kind: "tool_call",
      item_id: this.nextItemId("tool"),
      call_id: toolCall.callId,
      tool_name: toolCall.toolName,
      arguments: toolCall.arguments,
      ...(managedToolUse ? { managed_tool_use: managedToolUse } : {}),
    };
    this.items.push(item);
    this.enqueue({
      type: "tool_call",
      item_id: item.item_id,
      call_id: item.call_id,
      tool_name: item.tool_name,
      arguments: item.arguments,
      ...(item.managed_tool_use ? { managed_tool_use: item.managed_tool_use } : {}),
    });
  }

  async close(): Promise<void> {
    if (this.closing || this.disabled) return;
    this.closing = true;
    this.clearTimers();
    if (this.flushing) await this.flushing;
    const synchronized = await this.flushCurrent();
    if (!synchronized || this.sequence === 0) return;

    const closeSequence = this.sequence + 1;
    const response = await this.post({
      ...this.baseRequest(),
      type: "stream_closed",
      sequence: closeSequence,
    });
    if (response?.accepted) this.sequence = closeSequence;
  }

  private enqueue(update: LiveOutputUpdate): void {
    this.sequence += 1;
    this.pending.push({ sequence: this.sequence, update });
    this.scheduleFlush(this.batchDelayMs);
  }

  private nextItemId(prefix: string): string {
    this.itemSequence += 1;
    return `${prefix}_${this.itemSequence}`;
  }

  private clearTimers(): void {
    if (this.timer) clearTimeout(this.timer);
    if (this.retryTimer) clearTimeout(this.retryTimer);
    this.timer = undefined;
    this.retryTimer = undefined;
  }

  private scheduleFlush(delay: number): void {
    if (this.closing || this.disabled || this.timer || this.retryTimer) return;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      void this.flush().then((synchronized) => {
        if (!synchronized && !this.closing) this.scheduleRetry();
      });
    }, delay);
  }

  private scheduleRetry(): void {
    if (this.closing || this.disabled || this.retryTimer) return;
    this.retryTimer = setTimeout(() => {
      this.retryTimer = undefined;
      void this.flush().then((synchronized) => {
        if (!synchronized && !this.closing) this.scheduleRetry();
      });
    }, RETRY_DELAY_MS);
  }

  private async flush(): Promise<boolean> {
    if (this.flushing) return this.flushing;
    this.flushing = this.flushCurrent().finally(() => {
      this.flushing = undefined;
    });
    return this.flushing;
  }

  private async flushCurrent(): Promise<boolean> {
    if (this.sequence === 0) return true;
    const targetSequence = this.sequence;
    const response = this.needsSnapshot
      ? await this.post({
          ...this.baseRequest(),
          type: "snapshot",
          sequence: targetSequence,
          items: this.items,
        })
      : await this.postAppend(targetSequence);

    if (!response?.accepted) {
      this.needsSnapshot = true;
      return false;
    }

    const firstUnaccepted = this.pending.findIndex(
      (entry) => entry.sequence > response.accepted_sequence,
    );
    this.pending.splice(0, firstUnaccepted === -1 ? this.pending.length : firstUnaccepted);
    this.needsSnapshot = response.resync_required;
    if (this.pending.length > 0 && !this.closing) this.scheduleFlush(this.batchDelayMs);
    return !this.needsSnapshot;
  }

  private async postAppend(targetSequence: number): Promise<LiveOutputResponse | undefined> {
    const updates = this.pending.filter((entry) => entry.sequence <= targetSequence);
    if (updates.length === 0) return {
      accepted: true,
      accepted_sequence: targetSequence,
      resync_required: false,
    };
    return this.post({
      ...this.baseRequest(),
      type: "append",
      first_sequence: updates[0].sequence,
      updates: updates.map((entry) => entry.update),
    });
  }

  private baseRequest() {
    return {
      session_id: this.context.sessionId,
      turn_id: this.context.turnId,
      runtime_instance_id: this.context.runtimeInstanceId,
      stream_id: this.streamId,
    };
  }

  private async post(body: Record<string, unknown>): Promise<LiveOutputResponse | undefined> {
    try {
      const response = await this.fetchImpl(this.url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(5_000),
      });
      const parsed = asRecord(await parseJsonResponse(response));
      const acceptedSequence = parsed?.accepted_sequence;
      const accepted = parsed?.accepted;
      const resyncRequired = parsed?.resync_required;
      if (
        typeof acceptedSequence !== "number" ||
        !Number.isSafeInteger(acceptedSequence) ||
        acceptedSequence < 0 ||
        typeof accepted !== "boolean" ||
        typeof resyncRequired !== "boolean"
      ) {
        if (isPermanentRejection(response.status)) this.disabled = true;
        return undefined;
      }
      if (!response.ok && !resyncRequired) {
        if (isPermanentRejection(response.status)) this.disabled = true;
        return undefined;
      }
      return {
        accepted,
        accepted_sequence: acceptedSequence,
        resync_required: resyncRequired,
      };
    } catch {
      return undefined;
    }
  }
}

function isPermanentRejection(status: number): boolean {
  return status >= 400 && status < 500 && status !== 429;
}

function managedToolUseFor(toolCall: CompleteToolCall): ManagedToolUse | undefined {
  const input = asRecord(toolCall.arguments);
  if (!input) return undefined;

  switch (toolCall.toolName) {
    case "read": {
      const path = optionalString(input.path);
      if (!path) return undefined;
      const startLine = optionalNonNegativeInteger(input.start_line);
      const endLine = optionalNonNegativeInteger(input.end_line);
      return {
        tool_name: toolCall.toolName,
        input: {
          type: "read",
          path,
          ...(startLine !== undefined ? { start_line: startLine } : {}),
          ...(endLine !== undefined ? { end_line: endLine } : {}),
        },
      };
    }
    case "edit": {
      const path = optionalString(input.path);
      if (!path || !Array.isArray(input.edits)) return undefined;
      return {
        tool_name: toolCall.toolName,
        input: {
          type: "edit",
          path,
          edits_count: input.edits.length,
        },
      };
    }
    case "write": {
      const path = optionalString(input.path);
      return path ? { tool_name: toolCall.toolName, input: { type: "write", path } } : undefined;
    }
    case "bash": {
      const command = optionalString(input.command);
      if (!command) return undefined;
      const timeout = optionalNonNegativeInteger(input.timeout);
      return {
        tool_name: toolCall.toolName,
        input: {
          type: "bash",
          command,
          ...(timeout !== undefined ? { timeout } : {}),
        },
      };
    }
    default:
      return undefined;
  }
}

function optionalNonNegativeInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
}

export function completeToolCallFromMessageUpdate(event: unknown): CompleteToolCall | undefined {
  const streamEvent = asRecord(asRecord(event)?.assistantMessageEvent);
  if (streamEvent?.type !== "toolcall_end") return undefined;
  const toolCall = asRecord(streamEvent.toolCall);
  const callId = optionalString(toolCall?.id);
  const toolName = optionalString(toolCall?.name);
  if (!callId || !toolName || !("arguments" in (toolCall ?? {}))) return undefined;
  return { callId, toolName, arguments: toolCall!.arguments };
}
