import { RpcError } from "../shared/rpc-error.js";
import { readHistory, type HistoryContext } from "./history.js";
import {
  connectRuntime,
  type RuntimeConnection,
  type ControlHandler,
} from "../shared/control-socket.js";
import { MAX_CONTROL_FRAME_BYTES } from "../shared/control-socket.js";
export interface ControlInput {
  input: string;
  inboxMessageId?: string;
  steerTargetTurnId?: string;
}
export interface LifecycleControl {
  interrupt(): void;
  shutdown(): void | (() => void);
  shutdownIfIdle(): boolean | (() => void);
}
export interface CompactionControl {
  compact(operationId: string): void;
}

export type PiThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max";

export interface ReasoningEffortControl {
  setReasoningEffort(level: PiThinkingLevel): void;
}

const THINKING_LEVELS = new Set<PiThinkingLevel>([
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
]);

export interface PiControlBinding {
  socket?: RuntimeConnection;
  handle?: ControlHandler;
  reconnected?: () => Promise<void>;
  error?: (error: Error) => void;
}

export async function connectPi(
  pontiaHome: string,
  onError: (error: Error) => void,
  onSubmit: (input: ControlInput) => void | Promise<void>,
  onReconnect?: () => Promise<void>,
  onReplay?: (inboxMessageId: string) => void,
  lifecycle?: LifecycleControl,
  history?: () => HistoryContext,
  compaction?: CompactionControl,
  binding: PiControlBinding = {},
  reasoningEffort?: ReasoningEffortControl,
): Promise<RuntimeConnection> {
  binding.error = onError;
  binding.reconnected = onReconnect;
  binding.handle = (method, params, respond, error, frameBytes) => {
    if (method === "history.read" && history) {
      try {
        respond({ result: readHistory(history(), params) });
      } catch (failure) {
        error(
          failure instanceof RpcError ? failure.code : -32009,
          failure instanceof Error ? failure.message : String(failure),
        );
      }
      return;
    }
    if (method === "ping") {
      respond({ result: { pong: true } });
      return;
    }
    if (method === "compact" && compaction) {
      if (
        Object.keys(params).some((key) => key !== "operation_id") ||
        typeof params.operation_id !== "string" ||
        !params.operation_id.trim()
      ) {
        error(-32602, "compact requires an operation_id");
        return;
      }
      try {
        compaction.compact(params.operation_id);
        respond({ result: { accepted: true } });
      } catch (failure) {
        error(
          failure instanceof RpcError ? failure.code : -32006,
          failure instanceof Error ? failure.message : String(failure),
        );
      }
      return;
    }
    if (method === "set_reasoning_effort" && reasoningEffort) {
      if (
        Object.keys(params).some((key) => key !== "reasoning_effort") ||
        typeof params.reasoning_effort !== "string" ||
        !THINKING_LEVELS.has(params.reasoning_effort as PiThinkingLevel)
      ) {
        error(-32602, "set_reasoning_effort requires a Pi thinking level");
        return;
      }
      try {
        reasoningEffort.setReasoningEffort(params.reasoning_effort as PiThinkingLevel);
        respond({ result: { accepted: true } });
      } catch (failure) {
        error(
          failure instanceof RpcError ? failure.code : -32006,
          failure instanceof Error ? failure.message : String(failure),
        );
      }
      return;
    }
    if (lifecycle && method === "shutdown_if_idle") {
      try {
        const action = lifecycle.shutdownIfIdle();
        respond(
          { result: { accepted: action !== false } },
          typeof action === "function" ? action : undefined,
        );
      } catch (failure) {
        error(
          failure instanceof RpcError ? failure.code : -32006,
          failure instanceof Error ? failure.message : String(failure),
        );
      }
      return;
    }
    if (lifecycle && (method === "interrupt" || method === "shutdown")) {
      try {
        const action = lifecycle[method as "interrupt" | "shutdown"]();
        respond({ result: { accepted: true } }, action || undefined);
      } catch (failure) {
        error(
          failure instanceof RpcError ? failure.code : -32006,
          failure instanceof Error ? failure.message : String(failure),
        );
      }
      return;
    }
    if (method === "branch.replay" && onReplay) {
      if (typeof params.replay_id !== "string" || !/^replay_[^\s]+$/.test(params.replay_id)) {
        error(-32602, "branch.replay requires a replay identifier");
        return;
      }
      try {
        onReplay(params.replay_id);
        respond({ result: { accepted: true } });
      } catch (failure) {
        error(
          failure instanceof RpcError ? failure.code : -32006,
          failure instanceof Error ? failure.message : String(failure),
        );
      }
      return;
    }
    if (method !== "submit" && method !== "steer") {
      error(-32601, "Unknown Pi control method");
      return;
    }
    if (frameBytes > MAX_CONTROL_FRAME_BYTES) {
      error(-32602, "Pi submit frame exceeds 64 KiB");
      return;
    }
    if (
      typeof params.input !== "string" ||
      !params.input.trim() ||
      (method === "steer" && (typeof params.turn_id !== "string" || !params.turn_id)) ||
      (params.inbox_message_id != null &&
        (typeof params.inbox_message_id !== "string" || !params.inbox_message_id))
    ) {
      error(-32602, "submit requires non-empty input and optional inbox_message_id");
      return;
    }
    const submit = async () => {
      await onSubmit({
        input: params.input,
        inboxMessageId: params.inbox_message_id ?? undefined,
        ...(method === "steer" ? { steerTargetTurnId: params.turn_id } : {}),
      });
      respond({ result: { accepted: true } });
    };
    void submit()
      .catch((failure) => {
        error(
          failure instanceof RpcError ? failure.code : -32006,
          failure instanceof Error ? failure.message : String(failure),
        );
      })
      .catch((failure) => onError(failure));
  };
  binding.socket ??= await connectRuntime(
    pontiaHome,
    (error) => binding.error?.(error),
    (...args) => binding.handle?.(...args),
    async () => {
      await binding.reconnected?.();
    },
  );
  return binding.socket;
}
