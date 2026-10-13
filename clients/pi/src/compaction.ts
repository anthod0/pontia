import { randomUUID } from "node:crypto";
import type {
  SessionBeforeCompactEvent,
  SessionCompactEvent,
  SessionCompactFailedEvent,
  CompactionResult,
} from "@earendil-works/pi-coding-agent";
import type { InternalEvent } from "./events.js";
import type { SessionContext } from "./session.js";
import { RpcError } from "./rpc-error.js";

type Trigger = "manual" | "threshold" | "overflow" | "unknown";
interface Operation {
  id: string;
  trigger: Trigger;
  started?: InternalEvent;
  terminal?: InternalEvent;
  acknowledged: Set<string>;
  reporting: Promise<void>;
  awaitingNativeResult: boolean;
  failureKey?: string;
}
interface State {
  report: (event: InternalEvent) => Promise<boolean>;
  active?: Operation;
  operations: Map<string, Operation>;
  signals: WeakMap<AbortSignal, Operation>;
  failures: WeakSet<object>;
  completed: Map<string, Operation>;
}

// Pi reloads extensions in the same process. Keep only control metadata across
// reload, so a replaced hook or reconnect cannot allocate another operation.
const stateKey = Symbol.for("pontia.pi.compaction");
const processState = globalThis as typeof globalThis & { [stateKey]?: Map<string, State> };
const states = (processState[stateKey] ??= new Map());

export class PiCompaction {
  private readonly state: State;

  constructor(
    home: string,
    private readonly context: SessionContext,
    report: (event: InternalEvent) => Promise<boolean>,
  ) {
    const key = JSON.stringify([
      home,
      context.sessionId,
      context.runtimeId,
      context.clientSessionKey,
    ]);
    let state = states.get(key);
    if (!state) {
      state = {
        report,
        operations: new Map(),
        signals: new WeakMap(),
        failures: new WeakSet(),
        completed: new Map(),
      };
      states.set(key, state);
    }
    state.report = report;
    this.state = state;
  }

  get running(): boolean {
    return !!this.state.active && !this.state.active.terminal;
  }

  async reconnect(): Promise<void> {
    for (const operation of this.state.operations.values()) {
      if (operation.started) await this.publish(operation, !operation.awaitingNativeResult);
    }
  }

  request(operationId: string, compact: () => void): void {
    if (this.state.operations.has(operationId)) return;
    if (this.running) throw new RpcError(-32006, "Pi compaction is already running");
    const operation = this.create("manual", operationId);
    operation.awaitingNativeResult = true;
    try {
      compact();
    } catch (error) {
      this.state.operations.delete(operationId);
      this.state.active = undefined;
      throw error;
    }
  }

  async before(event: SessionBeforeCompactEvent): Promise<void> {
    let operation = this.state.signals.get(event.signal);
    if (!operation) {
      operation = this.running ? this.state.active! : this.create(trigger(event.reason));
      this.state.signals.set(event.signal, operation);
    }
    await this.publish(operation);
  }

  async completed(event: SessionCompactEvent): Promise<void> {
    const entry = event.compactionEntry;
    const operation = this.state.completed.get(entry.id) ?? this.current(event.reason);
    this.state.completed.set(entry.id, operation);
    operation.terminal ??= this.fact("session.compaction_completed", operation, {
      native_compaction_id: entry.id ?? null,
      tokens_before: tokens(entry.tokensBefore),
      // Pi's extension event does not expose a post-compaction token count.
      tokens_after: null,
    });
    // The manual context callback supplies Pi's estimatedTokensAfter result.
    // Automatic compaction exposes only the extension event, so it stays null.
    await this.publish(operation, !operation.awaitingNativeResult);
  }

  async failed(event: SessionCompactFailedEvent, leaf: string | null): Promise<void> {
    if (this.state.failures.has(event)) return;
    this.state.failures.add(event);
    const failureKey = JSON.stringify([
      event.reason,
      event.aborted,
      event.errorMessage ?? null,
      leaf,
    ]);
    const previous = this.state.active;
    const operation =
      this.running || previous?.failureKey === failureKey
        ? previous!
        : this.create(trigger(event.reason));
    operation.failureKey = failureKey;
    operation.terminal ??= this.fact("session.compaction_failed", operation, {
      aborted: event.aborted,
      error_message: event.errorMessage ?? null,
    });
    operation.awaitingNativeResult = false;
    await this.publish(operation);
  }

  // The callback covers native failures occurring before session_before_compact
  // (for example, a session too small to compact). Hooks remain authoritative
  // for cancellation and supply richer diagnostics when present.
  async callbackError(operationId: string, error: Error): Promise<void> {
    const operation = this.state.operations.get(operationId);
    if (!operation) return;
    operation.awaitingNativeResult = false;
    operation.terminal ??= this.fact("session.compaction_failed", operation, {
      aborted: false,
      error_message: error.message,
    });
    await this.publish(operation);
  }

  async callbackComplete(operationId: string, result: CompactionResult): Promise<void> {
    const operation = this.state.operations.get(operationId);
    if (!operation) return;
    operation.awaitingNativeResult = false;
    operation.terminal ??= this.fact("session.compaction_completed", operation, {
      native_compaction_id: null,
      tokens_before: tokens(result.tokensBefore),
      tokens_after: null,
    });
    if (
      !operation.acknowledged.has(operation.terminal.type) &&
      operation.terminal.type === "session.compaction_completed"
    ) {
      operation.terminal.data.tokens_after = tokens(result.estimatedTokensAfter);
    }
    await this.publish(operation);
  }

  private current(reason: unknown): Operation {
    return this.running ? this.state.active! : this.create(trigger(reason));
  }

  private create(reason: Trigger, id = `compact_${randomUUID()}`): Operation {
    const operation: Operation = {
      id,
      trigger: reason,
      acknowledged: new Set(),
      reporting: Promise.resolve(),
      awaitingNativeResult: false,
    };
    this.state.operations.set(id, operation);
    this.state.active = operation;
    return operation;
  }

  private fact(
    type: InternalEvent["type"],
    operation: Operation,
    metadata: Record<string, unknown> = {},
  ): InternalEvent {
    return {
      session_id: this.context.sessionId,
      type,
      data: { operation_id: operation.id, trigger: operation.trigger, ...metadata },
    };
  }

  private publish(operation: Operation, includeTerminal = true): Promise<void> {
    operation.started ??= this.fact("session.compaction_started", operation);
    operation.reporting = operation.reporting
      .catch(() => {})
      .then(async () => {
        for (const event of [
          operation.started!,
          includeTerminal ? operation.terminal : undefined,
        ]) {
          if (!event || operation.acknowledged.has(event.type)) continue;
          if (!(await this.state.report(event))) return;
          operation.acknowledged.add(event.type);
        }
      });
    return operation.reporting;
  }
}

function trigger(reason: unknown): Trigger {
  return reason === "manual" || reason === "threshold" || reason === "overflow"
    ? reason
    : "unknown";
}

function tokens(value: unknown): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : null;
}
