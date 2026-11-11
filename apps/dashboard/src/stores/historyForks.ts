import { get, writable } from "svelte/store";
import type { DeviceRuntime } from "../device/runtime";
import type { ForkSessionOutcome } from "../api/types";

export interface HistoryFork {
  operationId: string;
  sourceSessionId: string;
  turnId: string;
  input: string;
  sessionId: string | null;
  stage: ForkSessionOutcome["stage"] | "input_unconfirmed" | "input_submitted";
  error: string | null;
}

export function createHistoryForks(device: DeviceRuntime) {
  const storageKey = `pontia:history-forks:v1:${device.id}`;
  const saved = typeof localStorage === "undefined" ? null : localStorage.getItem(storageKey);
  const pending = writable<HistoryFork[]>(saved ? JSON.parse(saved) : []);

  function save(operation: HistoryFork): void {
    const operations = [
      ...get(pending).filter((item) => item.operationId !== operation.operationId),
      operation,
    ];
    localStorage.setItem(storageKey, JSON.stringify(operations));
    pending.set(operations);
  }

  function forget(operationId: string): void {
    const operations = get(pending).filter((item) => item.operationId !== operationId);
    localStorage.setItem(storageKey, JSON.stringify(operations));
    pending.set(operations);
  }

  async function resume(original: HistoryFork): Promise<HistoryFork> {
    let operation: HistoryFork = { ...original, error: null };
    try {
      if (operation.stage !== "session_created" && operation.stage !== "input_unconfirmed") {
        const result = await device.api.forkSession(
          operation.sourceSessionId,
          operation.turnId,
          operation.operationId,
        );
        operation = {
          ...operation,
          stage: result.stage,
          sessionId: result.session_id,
          error: result.error,
        };
        save(operation);
        if (result.stage !== "session_created") return operation;
      }
      operation.stage = "input_unconfirmed";
      save(operation);
      await device.api.submitInboxMessage(
        operation.sessionId!,
        {
          input: operation.input,
          delivery_policy: "after_idle",
          metadata: { source: "dashboard_history_fork" },
        },
        `fork-${operation.operationId}`,
      );
      operation.stage = "input_submitted";
      forget(operation.operationId);
    } catch (error) {
      operation.error = error instanceof Error ? error.message : String(error);
      save(operation);
    }
    return operation;
  }

  function start(sourceSessionId: string, turnId: string, input: string): Promise<HistoryFork> {
    const operation: HistoryFork = {
      operationId: crypto.randomUUID(),
      sourceSessionId,
      turnId,
      input,
      sessionId: null,
      stage: "fork_unconfirmed",
      error: null,
    };
    save(operation);
    return resume(operation);
  }

  return { pending, start, resume, forget };
}
