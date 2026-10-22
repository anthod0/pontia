import { randomUUID } from "node:crypto";
import type { TurnContext } from "./context.js";
import { MAX_RPC_FRAME_BYTES, RpcError, type PiConnection } from "./control-socket.js";
import { asRecord, optionalString } from "./values.js";

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

export interface LiveToolStatus {
  phase: "running" | "ended";
  outcome?: "succeeded" | "failed" | "rejected" | "interrupted";
}
export interface LiveToolOutput {
  text: string;
  truncated: boolean;
}
export interface LiveToolResult {
  content?: unknown;
  error?: unknown;
  truncated: boolean;
}

export type LiveOutputItem =
  | { kind: "assistant_text"; item_id: string; text: string }
  | {
      kind: "tool_call";
      item_id: string;
      call_id: string;
      tool_name: string;
      arguments: unknown;
      managed_tool_use?: ManagedToolUse;
      status?: LiveToolStatus;
      output?: LiveToolOutput;
      result?: LiveToolResult;
    };

export type LiveOutputUpdate =
  | { type: "tool_status_updated"; call_id: string; status: LiveToolStatus }
  | { type: "tool_result"; call_id: string; result: LiveToolResult }
  | { type: "tool_output_delta"; call_id: string; delta: string; truncated: boolean }
  | { type: "tool_output_updated"; call_id: string; text: string; truncated: boolean }
  | { type: "assistant_text_delta"; item_id: string; delta: string }
  | {
      type: "tool_call";
      item_id: string;
      call_id: string;
      tool_name: string;
      arguments: unknown;
      managed_tool_use?: ManagedToolUse;
    };

export interface CompleteToolCall {
  callId: string;
  toolName: string;
  arguments: unknown;
}

export interface LiveOutputPublisherLike {
  appendText(delta: string): void;
  appendToolCall(toolCall: CompleteToolCall): void;
  toolExecution(event: unknown): void;
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
  connection: Pick<PiConnection, "request">;
  streamId?: string;
  batchDelayMs?: number;
}

export class LiveOutputPublisher implements LiveOutputPublisherLike {
  private readonly connection: Pick<PiConnection, "request">;
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

  constructor(context: TurnContext & { turnId: string }, options: LiveOutputPublisherOptions) {
    this.context = context;
    this.connection = options.connection;
    this.streamId = options.streamId ?? `stream_${randomUUID()}`;
    this.batchDelayMs = options.batchDelayMs ?? DEFAULT_BATCH_DELAY_MS;
  }

  appendText(delta: string): void {
    if (this.closing || this.disabled || delta.length === 0) return;
    let item = this.items.at(-1);
    if (item?.kind !== "assistant_text") {
      if (this.items.length >= 256) return;
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
    )
      return;
    if (this.items.length >= 256) return;
    const managedToolUse = managedToolUseFor(toolCall);
    const inputBytes = jsonBytes([toolCall.arguments, managedToolUse ?? null]);
    if (inputBytes > 64 * 1024 || this.toolBytes() + inputBytes > 1024 * 1024) return;
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

  toolExecution(event: unknown): void {
    if (this.closing || this.disabled) return;
    const native = asRecord(event);
    const callId = optionalString(native?.toolCallId);
    if (!native || !callId) return;
    if (
      (native.type === "tool_execution_start" || native.type === "tool_execution_update") &&
      "args" in native
    ) {
      const toolName = optionalString(native.toolName);
      if (toolName) this.appendToolCall({ callId, toolName, arguments: native.args });
    }
    const item = this.items.find(
      (item): item is Extract<LiveOutputItem, { kind: "tool_call" }> =>
        item.kind === "tool_call" && item.call_id === callId,
    );
    if (!item) return;
    if (native.type === "tool_execution_start") {
      if (item.status) return;
      item.status = { phase: "running" };
      this.enqueue({ type: "tool_status_updated", call_id: callId, status: item.status });
    } else if (native.type === "tool_execution_update") {
      if (item.status?.phase === "ended" || !("partialResult" in native)) return;
      const partial = asRecord(native.partialResult);
      const content = partial?.content;
      const text = Array.isArray(content)
        ? content
            .map((block) =>
              typeof asRecord(block)?.text === "string"
                ? asRecord(block)!.text
                : JSON.stringify(block),
            )
            .join("\n")
        : typeof native.partialResult === "string"
          ? native.partialResult
          : JSON.stringify(native.partialResult);
      if (text === undefined) return;
      const budget = Math.max(
        0,
        1024 * 1024 - this.toolBytes() + escapedTextBytes(item.output?.text ?? ""),
      );
      const retained = truncateText(text, budget);
      const truncated = retained !== text || nativeTruncated(native.partialResult);
      if (item.output?.text === retained && item.output.truncated === truncated) return;
      item.output = { text: retained, truncated };
      this.enqueue({ type: "tool_output_updated", call_id: callId, ...item.output });
    } else if (native.type === "tool_execution_end") {
      if (native.result !== undefined) {
        const truncated = nativeTruncated(native.result);
        const result: LiveToolResult =
          native.isError === true
            ? { error: native.result, truncated }
            : { content: native.result, truncated };
        const budget = Math.max(0, 1024 * 1024 - this.toolBytes() + resultBytes(item.result));
        if (resultBytes(result) > budget) {
          const key = native.isError === true ? "error" : "content";
          const value = result[key];
          result[key] = truncateText(
            typeof value === "string" ? value : JSON.stringify(value),
            budget,
          );
          result.truncated = true;
        }
        if (JSON.stringify(item.result) !== JSON.stringify(result)) {
          item.result = result;
          this.enqueue({ type: "tool_result", call_id: callId, result });
        }
      }
      const status: LiveToolStatus = {
        phase: "ended",
        ...(typeof native.isError === "boolean"
          ? { outcome: native.isError ? ("failed" as const) : ("succeeded" as const) }
          : {}),
      };
      if (!item.status?.outcome && JSON.stringify(item.status) !== JSON.stringify(status)) {
        item.status = status;
        this.enqueue({ type: "tool_status_updated", call_id: callId, status });
      }
    }
  }

  private toolBytes(): number {
    return this.items.reduce(
      (size, item) =>
        item.kind === "tool_call"
          ? size +
            jsonBytes([item.arguments, item.managed_tool_use ?? null]) +
            escapedTextBytes(item.output?.text ?? "") +
            resultBytes(item.result)
          : size,
      0,
    );
  }

  async close(): Promise<void> {
    if (this.closing || this.disabled) return;
    this.closing = true;
    this.clearTimers();
    if (this.flushing) await this.flushing;
    const synchronized = await this.flushCurrent();
    if (!synchronized || this.sequence === 0) return;

    const closeSequence = this.sequence + 1;
    const response = await this.publish({
      ...this.baseRequest(),
      type: "stream_closed",
      sequence: closeSequence,
    });
    if (response?.accepted) this.sequence = closeSequence;
  }

  private enqueue(update: LiveOutputUpdate): void {
    this.sequence += 1;
    this.pending.push({ sequence: this.sequence, update: structuredClone(update) });
    // Bound retries during arbitrarily long disconnects. The snapshot covers
    // everything omitted from the delta queue.
    if (this.pending.length > 256 || jsonBytes(this.pending) > 1024 * 1024) {
      this.pending.length = 0;
      this.needsSnapshot = true;
    }
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
    this.boundSnapshotToFrame();
    const response = this.needsSnapshot
      ? await this.publish({
          ...this.baseRequest(),
          type: "snapshot",
          sequence: targetSequence,
          items: this.items,
        })
      : await this.publishAppend(targetSequence);

    if (!response?.accepted) {
      this.needsSnapshot = true;
      return false;
    }

    const firstUnaccepted = this.pending.findIndex(
      (entry) => entry.sequence > response.accepted_sequence,
    );
    this.pending.splice(0, firstUnaccepted === -1 ? this.pending.length : firstUnaccepted);
    this.needsSnapshot =
      response.resync_required ||
      (this.sequence > response.accepted_sequence &&
        this.pending[0]?.sequence !== response.accepted_sequence + 1);
    if ((this.pending.length > 0 || this.needsSnapshot) && !this.closing)
      this.scheduleFlush(this.batchDelayMs);
    return !this.needsSnapshot;
  }

  private boundSnapshotToFrame(): void {
    const excess = () =>
      jsonBytes({
        jsonrpc: "2.0",
        id: `pi:${Number.MAX_SAFE_INTEGER}`,
        method: "liveOutput.publish",
        params: {
          ...this.baseRequest(),
          type: "snapshot",
          sequence: this.sequence,
          items: this.items,
        },
      }) - MAX_RPC_FRAME_BYTES;
    if (excess() <= 0) return;
    let changed = false;
    for (const item of this.items) {
      if (item.kind !== "tool_call") continue;
      if (item.output && excess() > 0 && item.output.text) {
        item.output.text = truncateText(
          item.output.text,
          Math.max(0, escapedTextBytes(item.output.text) - excess()),
        );
        item.output.truncated = true;
        changed = true;
      }
      if (item.result) {
        for (const key of ["content", "error"] as const) {
          if (!(key in item.result) || excess() <= 0) continue;
          const value = item.result[key];
          const budget = Math.max(0, jsonBytes(value) - excess() - 2);
          item.result[key] = truncateText(
            typeof value === "string" ? value : JSON.stringify(value),
            budget,
          );
          item.result.truncated = true;
          changed = true;
        }
      }
    }
    if (changed) {
      this.pending.length = 0;
      this.needsSnapshot = true;
    }
  }

  private async publishAppend(targetSequence: number): Promise<LiveOutputResponse | undefined> {
    const updates = this.pending.filter((entry) => entry.sequence <= targetSequence);
    if (updates.length === 0)
      return {
        accepted: true,
        accepted_sequence: targetSequence,
        resync_required: false,
      };
    return this.publish({
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
      runtime_id: this.context.runtimeId,
      stream_id: this.streamId,
    };
  }

  private async publish(body: Record<string, unknown>): Promise<LiveOutputResponse | undefined> {
    try {
      // Freeze the batch before a reconnect can delay its serialization.
      const response = await this.connection.request("liveOutput.publish", structuredClone(body));
      const parsed = asRecord(response);
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
        return undefined;
      }
      return {
        accepted,
        accepted_sequence: acceptedSequence,
        resync_required: resyncRequired,
      };
    } catch (error) {
      if (error instanceof RpcError && error.code !== -32603) this.disabled = true;
      return undefined;
    }
  }
}

function nativeTruncated(value: unknown): boolean {
  const result = asRecord(value);
  const details = asRecord(result?.details);
  return (
    result?.truncated === true ||
    details?.truncated === true ||
    asRecord(details?.truncation)?.truncated === true ||
    asRecord(result?.structuredContent)?.truncated === true
  );
}

function byteLength(text: string): number {
  return Buffer.byteLength(text, "utf8");
}
function jsonBytes(value: unknown): number {
  return byteLength(JSON.stringify(value) ?? "");
}
function resultBytes(result?: LiveToolResult): number {
  if (!result) return 0;
  return ["content", "error"].reduce((size, key) => {
    if (!(key in result)) return size;
    const value = result[key as "content" | "error"];
    return size + (typeof value === "string" ? escapedTextBytes(value) : jsonBytes(value));
  }, 0);
}
function escapedTextBytes(text: string): number {
  return jsonBytes(text) - 2;
}
function truncateText(text: string, budget: number): string {
  if (escapedTextBytes(text) <= budget) return text;
  let end = 0;
  let bytes = 0;
  for (const char of text) {
    bytes += escapedTextBytes(char);
    if (bytes > budget) break;
    end += char.length;
  }
  return text.slice(0, end);
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
