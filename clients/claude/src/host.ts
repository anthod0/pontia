import {
  query,
  type Options,
  type Query,
  type SDKMessage,
  type SDKAssistantMessage,
  type SDKPartialAssistantMessage,
} from "@anthropic-ai/claude-agent-sdk";
import { Compaction } from "./compaction.js";
import { Input } from "./input.js";
import { Interactions } from "./interactions.js";
import { FactError, InputFacts, type Emit, type ResultDiagnostic } from "./facts.js";

export interface ExecutionPolicy {
  tools: string[] | null;
  disabled_tools: string[];
  disable_mcp: boolean;
  disable_skills: boolean;
  yolo: boolean;
}

export type QueryFactory = (
  args: Parameters<typeof query>[0],
) => Pick<
  Query,
  typeof Symbol.asyncIterator | "interrupt" | "close" | "supportedModels" | "getContextUsage"
>;
export class ControlError extends Error {
  constructor(
    readonly code: "rejected" | "unknown" | "unsupported",
    readonly reason: string,
  ) {
    super(reason);
  }
}

export class Host {
  private readonly input = new Input();
  private readonly compaction: Compaction;
  private readonly interactions: Interactions;
  private execution?: ReturnType<QueryFactory>;
  private reader?: Promise<void>;
  private current?: InputFacts;
  private native?: string;
  private unavailable?: string;
  private retrying = false;
  private usageRevision = 0;
  private contextModel?: string;
  private usageMessageId?: string;
  private streamUsage?: SDKAssistantMessage["message"]["usage"];
  private contextSummary?: ReturnType<Query["getContextUsage"]>;
  private controlConnected = true;
  private closing = false;
  private closed = false;
  private usedInputs = new Set<string>();

  constructor(
    private readonly options: {
      session_id: string;
      cwd: string;
      model?: string;
      resume?: string;
      system_prompt?: string;
      execution_policy?: ExecutionPolicy;
    },
    private readonly emit: Emit,
    private readonly createQuery: QueryFactory = query,
  ) {
    this.interactions = new Interactions(emit);
    this.compaction = new Compaction(emit);
  }

  status() {
    return {
      native_session_id: this.native ?? null,
      input_id: this.current?.id ?? null,
      started: this.current?.started ?? false,
      finished: this.current?.finished ?? false,
      unavailable: this.unavailable ?? (this.controlConnected ? null : "control_connection_lost"),
      closing: this.closing,
      closed: this.closed,
    };
  }

  isIdle(): boolean {
    if (!this.acceptsControl()) throw new ControlError("rejected", "execution_unavailable");
    return !this.compaction.busy && (!this.current || this.current.finished);
  }

  submit(id: string, text: string): void {
    if (!this.acceptsControl()) throw new ControlError("rejected", "execution_unavailable");
    this.validateInput(id, text);
    if (!this.isIdle()) throw new ControlError("rejected", "input_busy");
    this.usedInputs.add(id);
    this.current = new InputFacts(id, this.emit);
    this.input.push({
      type: "user",
      uuid: id as `${string}-${string}-${string}-${string}-${string}`,
      session_id: this.native ?? this.options.resume ?? "",
      parent_tool_use_id: null,
      message: { role: "user", content: text },
    });
    if (!this.execution) {
      const policy = this.options.execution_policy;
      const tools = policy?.tools;
      const options: Options = {
        cwd: this.options.cwd,
        ...(this.options.model ? { model: this.options.model } : {}),
        ...(this.options.resume ? { resume: this.options.resume } : {}),
        ...(this.options.system_prompt
          ? {
              systemPrompt: {
                type: "preset" as const,
                preset: "claude_code" as const,
                append: this.options.system_prompt,
              },
            }
          : {}),
        includePartialMessages: true,
        permissionMode: policy?.yolo ? "bypassPermissions" : "default",
        ...(policy?.yolo
          ? { allowDangerouslySkipPermissions: true, sandbox: { enabled: false } }
          : {}),
        ...(tools ? { tools } : {}),
        ...(policy?.disable_mcp ? { strictMcpConfig: true, mcpServers: {} } : {}),
        ...(policy?.disable_skills ? { extraArgs: { "disable-slash-commands": null } } : {}),
        settingSources: ["user", "project", "local"],
        settings: { env: { PONTIA_SESSION_ID: this.options.session_id } },
        disallowedTools: [
          ...(policy?.yolo ? ["AskUserQuestion"] : []),
          ...(policy?.disabled_tools ?? []),
          ...(policy?.disable_mcp ? ["mcp__*"] : []),
        ],
        canUseTool: (tool, input, context) => {
          if (this.closed || this.closing || this.unavailable)
            return Promise.resolve({ behavior: "deny", message: "Tool approval is unavailable." });
          if (policy?.yolo) return Promise.resolve({ behavior: "allow", updatedInput: input });
          return this.interactions.request(tool, input, context);
        },
        onElicitation: (request, context) => {
          if (policy?.yolo) return Promise.resolve({ action: "decline" });
          if (this.closed || this.closing || this.unavailable)
            return Promise.resolve({ action: "cancel" });
          return this.interactions.elicit(request, context);
        },
        // Raw stderr may include credentials or tool arguments. It is not a control fact.
        stderr: () => {},
      };
      try {
        this.execution = this.createQuery({ prompt: this.input, options });
        this.reader = this.consume(this.execution);
      } catch {
        this.fail("sdk_start_failed");
        throw new ControlError("unknown", "sdk_start_failed");
      }
    }
  }

  compact(operationId: string): void {
    if (!this.acceptsControl() || !this.execution || !this.native)
      throw new ControlError("rejected", "execution_unavailable");
    if (!this.isIdle()) throw new ControlError("rejected", "input_busy");
    const id = this.compaction.request(operationId);
    this.input.push({
      type: "user",
      uuid: id as `${string}-${string}-${string}-${string}-${string}`,
      session_id: this.native,
      parent_tool_use_id: null,
      message: { role: "user", content: "/compact" },
    });
  }

  steer(target: string, id: string, text: string): void {
    if (!this.acceptsControl()) throw new ControlError("rejected", "execution_unavailable");
    this.validateInput(id, text);
    if (!this.current?.started || this.current.finished || this.current.id !== target)
      throw new ControlError("rejected", "no_matching_execution");
    this.input.push({
      type: "user",
      uuid: id as `${string}-${string}-${string}-${string}-${string}`,
      session_id: this.native!,
      parent_tool_use_id: null,
      message: { role: "user", content: text },
      priority: "now",
    });
    this.usedInputs.add(id);
    this.current.steer(id, text);
  }

  private validateInput(id: string, text: string): void {
    if (
      !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id) ||
      !text.trim()
    ) {
      throw new ControlError("rejected", "invalid_input");
    }
    if (this.usedInputs.has(id)) throw new ControlError("rejected", "input_already_submitted");
  }

  async interrupt(id: string): Promise<void> {
    if (
      this.closed ||
      this.closing ||
      !this.execution ||
      !this.current?.started ||
      this.current.finished ||
      this.current.id !== id
    )
      throw new ControlError("rejected", "no_matching_execution");
    try {
      await this.execution.interrupt();
    } catch {
      throw new ControlError("unknown", "interrupt_unconfirmed");
    }
    // The result, not this acknowledgement, decides whether interruption won.
  }

  hasInteraction(id: string): boolean {
    return this.interactions.has(id);
  }

  cancelInteraction(id: string): void {
    this.interactions.cancel(id);
  }

  resolveInteraction(id: string, option: string): void {
    if (!this.acceptsControl() || !this.interactions.resolve(id, option))
      throw new ControlError("rejected", "permission_expired_or_invalid");
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closing = true;
    this.interactions.expire();
    if (this.execution) {
      try {
        this.execution.close();
      } catch {
        throw new ControlError("unknown", "close_unconfirmed");
      }
      await this.reader;
      if (!this.closed) throw new ControlError("unknown", "close_unconfirmed");
    } else {
      this.closed = true;
      this.emit({ kind: "closed" });
    }
    this.input.end();
  }

  controlLost(): void {
    this.controlConnected = false;
  }

  controlRestored(): void {
    this.controlConnected = true;
  }

  private acceptsControl(): boolean {
    return !this.closed && !this.closing && !this.unavailable && this.controlConnected;
  }

  private fail(reason: string, result?: ResultDiagnostic): void {
    if (this.unavailable) return;
    this.unavailable = reason;
    this.compaction.closed(false);
    this.interactions.expire();
    this.emit({ kind: "unavailable", reason, ...(result ? { result } : {}) });
  }

  private async consume(execution: ReturnType<QueryFactory>): Promise<void> {
    try {
      for await (const message of execution) {
        try {
          this.message(message);
        } catch (error) {
          this.fail(
            error instanceof Error ? error.message : "sdk_message_invalid",
            error instanceof FactError ? error.result : undefined,
          );
        }
      }
      if (this.closing) {
        this.compaction.closed(true);
        this.closed = true;
        this.emit({ kind: "closed", history_anchor: this.current?.historyAnchor });
      } else {
        this.fail("sdk_ended_without_close");
      }
    } catch {
      this.fail("sdk_reader_failed");
    }
  }

  private async reportContextUsage(usage?: SDKAssistantMessage["message"]["usage"]): Promise<void> {
    const revision = ++this.usageRevision;
    try {
      // /context measures against the resolved compaction window, including policy overrides.
      if (!usage || !this.contextSummary)
        this.contextSummary = this.execution!.getContextUsage({ detail: "summary" });
      const { rawMaxTokens: max, totalTokens } = await this.contextSummary;
      if (revision !== this.usageRevision || this.closing || this.unavailable) return;
      const cache =
        (usage?.cache_creation_input_tokens ?? 0) + (usage?.cache_read_input_tokens ?? 0);
      const used = usage ? usage.input_tokens + cache : totalTokens;
      if (!Number.isSafeInteger(max) || max <= 0) {
        this.contextSummary = undefined;
        return;
      }
      if (!Number.isSafeInteger(used) || used < 0) return;
      this.emit({
        kind: "context_usage",
        context_usage: {
          used_tokens: used,
          max_tokens: max,
          remaining_tokens: Math.max(0, max - used),
          usage_ratio: Math.min(1, used / max),
          ...(usage
            ? {
                confidence: "exact",
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_tokens: cache,
              }
            : { confidence: "estimated" }),
        },
      });
    } catch {
      if (revision !== this.usageRevision) return;
      this.contextSummary = undefined;
      if (!this.closing) console.error("Claude context usage query failed");
    }
  }

  private observeReply(message: SDKAssistantMessage | SDKPartialAssistantMessage): void {
    if (message.parent_tool_use_id) return;
    if (message.type === "assistant") {
      if (
        message.agent_id ||
        message.error ||
        message.message.model === "<synthetic>" ||
        message.message.id === this.usageMessageId ||
        !message.message.usage
      )
        return;
      this.usageMessageId = message.message.id;
      void this.reportContextUsage(message.message.usage);
      return;
    }
    const event = message.event;
    if (event.type === "message_start") {
      this.usageMessageId = event.message.id;
      this.streamUsage = event.message.usage;
    }
    if (event.type === "message_delta" && this.streamUsage) {
      // Streamed assistant envelopes carry the initial counts. The delta finalizes this request.
      const initial = this.streamUsage;
      this.streamUsage = undefined;
      const final = event.usage;
      void this.reportContextUsage({
        ...initial,
        input_tokens: final.input_tokens ?? initial.input_tokens,
        cache_creation_input_tokens:
          final.cache_creation_input_tokens ?? initial.cache_creation_input_tokens,
        cache_read_input_tokens: final.cache_read_input_tokens ?? initial.cache_read_input_tokens,
        output_tokens: final.output_tokens ?? initial.output_tokens,
      });
    }
  }

  private message(message: SDKMessage): void {
    if (this.unavailable) return;
    if (message.type === "system" && message.subtype === "init") {
      if (
        !message.session_id ||
        (this.native && message.session_id !== this.native) ||
        (this.options.resume && message.session_id !== this.options.resume)
      )
        throw new Error("native_identity_mismatch");
      if (!this.native) {
        this.native = message.session_id;
        this.emit({ kind: "ready", native_session_id: this.native });
      }
      this.emit({ kind: "model", model: message.model });
      if (this.contextModel !== message.model) {
        this.contextModel = message.model;
        void this.reportContextUsage();
      }
      return;
    }
    if (message.type === "system" && message.subtype === "api_retry") {
      if (!this.native || message.session_id !== this.native)
        throw new Error("native_identity_mismatch");
      this.retrying = true;
      this.emit({
        kind: "retry",
        reason:
          message.error === "authentication_failed"
            ? "authentication_failed_retrying"
            : "api_request_retrying",
      });
      return;
    }
    if (
      message.type === "system" &&
      (message.subtype === "status" || message.subtype === "compact_boundary")
    ) {
      if (!this.native || message.session_id !== this.native)
        throw new Error("native_identity_mismatch");
      this.compaction.accept(message);
      if (
        message.subtype === "compact_boundary" &&
        !("parent_tool_use_id" in message && message.parent_tool_use_id)
      ) {
        this.streamUsage = undefined;
        void this.reportContextUsage();
      }
      return;
    }
    if (["stream_event", "assistant", "result", "user"].includes(message.type)) {
      if (!this.native || message.session_id !== this.native)
        throw new Error("native_identity_mismatch");
      if (this.retrying && message.type !== "user") {
        this.retrying = false;
        this.emit({ kind: "retry", reason: null });
      }
      if (this.compaction.accept(message)) return;
      this.current?.accept(message);
      if (message.type === "assistant" || message.type === "stream_event")
        this.observeReply(message);
    }
  }
}
