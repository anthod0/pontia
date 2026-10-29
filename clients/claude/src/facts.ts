import type { SDKMessage, SDKResultMessage } from "@anthropic-ai/claude-agent-sdk";

export type HostEvent =
  | { kind: "ready"; native_session_id: string }
  | { kind: "started"; input_id: string }
  | { kind: "text"; input_id: string; item_id: string; delta: string }
  | { kind: "tool"; input_id: string; call_id: string; name: string; arguments: unknown }
  | { kind: "tool_result"; input_id: string; call_id: string; failed: boolean; content: unknown }
  | {
      kind: "finished";
      input_id: string;
      result_id: string;
      outcome: "completed" | "failed" | "interrupted";
    }
  | { kind: "unavailable"; reason: string; result?: ResultDiagnostic }
  | { kind: "permission_requested"; permission_id: string; tool: string; blocking: boolean }
  | { kind: "permission_settled"; permission_id: string; outcome: "resolved" | "expired" }
  | { kind: "closed" };

export type Emit = (event: HostEvent) => void;
export type ResultDiagnostic = Pick<
  SDKResultMessage,
  "subtype" | "is_error" | "stop_reason" | "terminal_reason"
>;
export class FactError extends Error {
  readonly result: ResultDiagnostic;
  constructor(reason: string, message: SDKResultMessage) {
    super(reason);
    this.result = {
      subtype: message.subtype,
      is_error: message.is_error,
      stop_reason: message.stop_reason,
      terminal_reason: message.terminal_reason,
    };
  }
}

function outcome(message: SDKResultMessage): "completed" | "failed" | "interrupted" | undefined {
  if (
    message.subtype === "success" &&
    !message.is_error &&
    message.stop_reason === "end_turn" &&
    message.terminal_reason === "completed"
  )
    return "completed";
  if (
    message.subtype === "success" &&
    message.is_error &&
    message.stop_reason === "stop_sequence" &&
    message.terminal_reason === "api_error"
  )
    return "failed";
  if (
    message.subtype === "error_during_execution" &&
    message.is_error &&
    ((message.stop_reason === null && message.terminal_reason === "aborted_streaming") ||
      (message.stop_reason === "tool_use" && message.terminal_reason === "aborted_tools"))
  )
    return "interrupted";
  return undefined;
}

function association(
  message: Pick<SDKResultMessage, "user_message_uuid" | "user_message_uuids">,
  input: string,
): "none" | "matched" | "conflict" {
  const ids =
    message.user_message_uuids ?? (message.user_message_uuid ? [message.user_message_uuid] : []);
  if (!ids.length) return message.user_message_uuid ? "conflict" : "none";
  return ids.length === 1 &&
    ids[0] === input &&
    (!message.user_message_uuid || message.user_message_uuid === input)
    ? "matched"
    : "conflict";
}

// One user input, not one model/tool round. No raw SDK messages leave this module.
export class InputFacts {
  started = false;
  finished = false;
  private ownsReplies = false;
  private streamMessage?: string;
  private streamed = new Set<string>();
  private complete = new Set<string>();
  private tools = new Set<string>();

  constructor(
    readonly id: string,
    private emit: Emit,
  ) {}

  accept(message: SDKMessage): void {
    if ("parent_tool_use_id" in message && message.parent_tool_use_id) return;
    if (message.type === "result") {
      const match = association(message, this.id);
      if (match === "none") return; // An automatic/background result does not consume our task.
      this.ownsReplies = false;
      this.streamMessage = undefined;
      if (match === "conflict") {
        throw new FactError("input_association_conflict", message);
      }
      if (this.finished) return;
      if (!this.started) throw new FactError("result_without_start", message);
      const terminal = outcome(message);
      if (!terminal) throw new FactError("unsupported_result", message);
      this.finished = true;
      this.emit({
        kind: "finished",
        input_id: this.id,
        result_id: message.uuid,
        outcome: terminal,
      });
      return;
    }
    if (this.finished) return;
    if (message.type === "stream_event" || message.type === "assistant") {
      const match = association(message, this.id);
      if (match === "conflict") throw new Error("input_association_conflict");
      if (match === "matched") {
        this.ownsReplies = true;
        if (!this.started) {
          this.started = true;
          this.emit({ kind: "started", input_id: this.id });
        }
      }
      if (!this.ownsReplies) return;
      if (message.type === "stream_event") {
        const event = message.event;
        if (event.type === "message_start") this.streamMessage = event.message.id;
        if (this.streamMessage) {
          if (event.type === "content_block_start" && event.content_block.type === "text") {
            this.streamed.add(this.streamMessage);
            this.text(`${this.streamMessage}:${event.index}`, event.content_block.text);
          }
          if (event.type === "content_block_delta" && event.delta.type === "text_delta") {
            this.streamed.add(this.streamMessage);
            this.text(`${this.streamMessage}:${event.index}`, event.delta.text);
          }
        }
        if (event.type === "message_stop") this.streamMessage = undefined;
      } else {
        const reply = message.message;
        // The SDK emits distinct block envelopes sharing the API message ID.
        if (this.complete.has(message.uuid)) return;
        this.complete.add(message.uuid);
        for (const [index, block] of reply.content.entries()) {
          if (block.type === "text" && !this.streamed.has(reply.id))
            this.text(`${message.uuid}:${index}`, block.text);
          if (block.type === "tool_use" && !this.tools.has(block.id)) {
            this.tools.add(block.id);
            this.emit({
              kind: "tool",
              input_id: this.id,
              call_id: block.id,
              name: block.name,
              arguments: block.input,
            });
          }
        }
      }
    }
    if (message.type === "user" && Array.isArray(message.message.content)) {
      for (const block of message.message.content) {
        if (block.type === "tool_result" && this.tools.delete(block.tool_use_id)) {
          this.emit({
            kind: "tool_result",
            input_id: this.id,
            call_id: block.tool_use_id,
            failed: block.is_error === true,
            content: block.content ?? null,
          });
        }
      }
    }
  }

  private text(item_id: string, delta: string): void {
    if (delta) this.emit({ kind: "text", input_id: this.id, item_id, delta });
  }
}
