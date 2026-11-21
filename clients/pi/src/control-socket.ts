import { RpcError } from "../shared/rpc-error.js";
import { readHistory, type HistoryContext } from "./history.js";
import { connectRuntime, type RuntimeConnection } from "../shared/control-socket.js";
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

export function connectPi(
  pontiaHome: string,
  onError: (error: Error) => void,
  onSubmit: (input: ControlInput) => void | Promise<void>,
  onReconnect?: () => Promise<void>,
  onReplay?: (inboxMessageId: string) => void,
  lifecycle?: LifecycleControl,
  history?: () => HistoryContext,
  compaction?: CompactionControl,
): Promise<RuntimeConnection> {
  return connectRuntime(
    pontiaHome,
    onError,
    (method, params, respond, error, frameBytes) => {
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
        if (
          typeof params.inbox_message_id !== "string" ||
          !/^msg_[^\s]+$/.test(params.inbox_message_id)
        ) {
          error(-32602, "branch.replay requires an Inbox Message identifier");
          return;
        }
        try {
          onReplay(params.inbox_message_id);
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
    },
    onReconnect,
  );
}
