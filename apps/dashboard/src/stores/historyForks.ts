import { get, writable } from "svelte/store";
import { readStored, scopedKey, writeStored } from "$lib/storage";
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
  const storageKey = scopedKey("pontia:history-forks:v1", device.id);
  // Unreadable storage or data throws instead of reading as empty, so a later save cannot drop forks.
  const saved = readStored(storageKey, { required: true });
  const pending = writable<HistoryFork[]>(saved ? JSON.parse(saved) : []);

  // A failed write throws, so a fork that cannot be saved does not proceed.
  function persist(operations: HistoryFork[]): void {
    writeStored(storageKey, JSON.stringify(operations), { required: true });
    pending.set(operations);
  }

  function save(operation: HistoryFork): void {
    const operations = [
      ...get(pending).filter((item) => item.operationId !== operation.operationId),
      operation,
    ];
    persist(operations);
  }

  function forget(operationId: string): void {
    persist(get(pending).filter((item) => item.operationId !== operationId));
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
      await device.api.sendMessage(
        operation.sessionId!,
        {
          input: operation.input,
          mode: "queue",
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
