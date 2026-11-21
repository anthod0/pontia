import { randomUUID } from "node:crypto";
import type { SDKMessage } from "@anthropic-ai/claude-agent-sdk";
import type { Emit, HostEvent } from "./facts.js";

type CompactionFact = Extract<HostEvent, { kind: "compaction" }>;
interface Operation {
  id: string;
  trigger: "manual" | "unknown";
  input?: string;
  started: boolean;
  terminal?: CompactionFact;
}

export class Compaction {
  private active?: Operation;
  private completed = new Set<string>();

  constructor(private readonly emit: Emit) {}

  get busy(): boolean {
    return this.active !== undefined;
  }

  request(operationId: string): string {
    const input = randomUUID();
    this.active = { id: operationId, trigger: "manual", input, started: false };
    return input;
  }

  accept(message: SDKMessage): boolean {
    if ("parent_tool_use_id" in message && message.parent_tool_use_id) return false;
    if (message.type === "system" && message.subtype === "status") {
      if (message.status === "compacting") this.start();
      if (message.compact_result === "failed" && this.active) {
        this.fail(false, "Claude context compaction failed.");
        if (!this.active.input) this.finish();
      }
    }
    if (message.type === "system" && message.subtype === "compact_boundary") {
      if (this.completed.has(message.uuid)) return true;
      this.completed.add(message.uuid);
      const operation = this.start();
      operation.terminal = this.fact("completed", {
        native_compaction_id: message.uuid,
        tokens_before: tokens(message.compact_metadata.pre_tokens),
        tokens_after: tokens(message.compact_metadata.post_tokens),
      });
      if (!operation.input) this.finish();
      return true;
    }
    if (!this.active?.input) return false;
    // /compact produces local user frames and a model-less result, not a Pontia Turn.
    if (message.type === "result") {
      const ids = message.user_message_uuids ?? [message.user_message_uuid];
      if (
        !ids.includes(this.active.input) ||
        ids.some((id) => id !== this.active!.input) ||
        (message.user_message_uuid && message.user_message_uuid !== this.active.input)
      )
        throw new Error("compaction_result_unassociated");
      if (!this.active.terminal) {
        const aborted =
          message.terminal_reason === "aborted_streaming" ||
          message.terminal_reason === "aborted_tools";
        this.fail(
          aborted,
          message.is_error
            ? "Claude context compaction failed."
            : "Claude did not compact the context; there may be too few messages to summarize.",
        );
      }
      this.finish();
    }
    return true;
  }

  closed(aborted: boolean): void {
    if (!this.active) return;
    this.fail(aborted, "Claude stopped before context compaction finished.");
    this.finish();
  }

  private start(): Operation {
    this.active ??= { id: `compact_${randomUUID()}`, trigger: "unknown", started: false };
    if (!this.active.started) {
      this.active.started = true;
      this.emit(this.fact("started"));
    }
    return this.active;
  }

  private fact(
    phase: CompactionFact["phase"],
    metadata: Record<string, unknown> = {},
  ): CompactionFact {
    return {
      kind: "compaction",
      phase,
      data: {
        operation_id: this.active!.id,
        trigger: this.active!.trigger,
        ...metadata,
      },
    };
  }

  private fail(aborted: boolean, error: string): void {
    const operation = this.start();
    operation.terminal ??= this.fact("failed", { aborted, error_message: error });
  }

  private finish(): void {
    const terminal = this.active!.terminal!;
    this.active = undefined;
    this.emit(terminal);
  }
}

function tokens(value: number | undefined): number | null {
  return value !== undefined && Number.isSafeInteger(value) && value >= 0 ? value : null;
}
