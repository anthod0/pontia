import { query, type Options, type Query, type SDKMessage } from "@anthropic-ai/claude-agent-sdk";
import { Input } from "./input.js";
import { FactError, InputFacts, type Emit, type ResultDiagnostic } from "./facts.js";

export type QueryFactory = (
  args: Parameters<typeof query>[0],
) => Pick<Query, typeof Symbol.asyncIterator | "interrupt" | "close">;
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
  private execution?: ReturnType<QueryFactory>;
  private reader?: Promise<void>;
  private current?: InputFacts;
  private native?: string;
  private unavailable?: string;
  private controlConnected = true;
  private closing = false;
  private closed = false;
  private usedInputs = new Set<string>();

  constructor(
    private readonly options: { cwd: string; resume?: string },
    private readonly emit: Emit,
    private readonly createQuery: QueryFactory = query,
  ) {}

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

  submit(id: string, text: string): void {
    if (this.closed || this.closing || this.unavailable || !this.controlConnected)
      throw new ControlError("rejected", "execution_unavailable");
    if (
      !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id) ||
      !text.trim()
    ) {
      throw new ControlError("rejected", "invalid_input");
    }
    if (this.usedInputs.has(id)) throw new ControlError("rejected", "input_already_submitted");
    if (this.current && !this.current.finished) throw new ControlError("rejected", "input_busy");
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
      const options: Options = {
        cwd: this.options.cwd,
        ...(this.options.resume ? { resume: this.options.resume } : {}),
        includePartialMessages: true,
        permissionMode: "default",
        settingSources: ["user", "project", "local"],
        disallowedTools: ["AskUserQuestion"],
        canUseTool: async () => ({
          behavior: "deny",
          message: "Pontia tool approval is not available.",
        }),
        onElicitation: async () => ({ action: "decline" }),
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

  async close(): Promise<void> {
    if (this.closed) return;
    this.closing = true;
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

  controlRestored(factsCaughtUp: boolean): void {
    // No replay buffer: only a controller that has already ingested every emitted
    // fact can resume input. Before the first input, there cannot be a fact gap.
    this.controlConnected = !this.execution || factsCaughtUp;
  }

  private fail(reason: string, result?: ResultDiagnostic): void {
    if (this.unavailable) return;
    this.unavailable = reason;
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
        this.closed = true;
        this.emit({ kind: "closed" });
      } else {
        this.fail("sdk_ended_without_close");
      }
    } catch {
      this.fail("sdk_reader_failed");
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
      return;
    }
    if (["stream_event", "assistant", "result", "user"].includes(message.type)) {
      if (!this.native || message.session_id !== this.native)
        throw new Error("native_identity_mismatch");
      this.current?.accept(message);
    }
  }
}
