import { beforeEach, expect, test, vi } from "vitest";
import { get } from "svelte/store";
import { createHistoryForks } from "../../src/stores/historyForks";
import type { DeviceRuntime } from "../../src/device/runtime";

const api = { forkSession: vi.fn(), sendMessage: vi.fn() };
const device = { id: "fork-test", api } as unknown as DeviceRuntime;

beforeEach(() => {
  localStorage.clear();
  vi.resetAllMocks();
});

test("a lost input receipt survives reload and retries the same child and input identity", async () => {
  api.forkSession.mockResolvedValue({ stage: "session_created", session_id: "child", error: null });
  api.sendMessage.mockRejectedValueOnce(new TypeError("response lost"));
  const first = createHistoryForks(device);
  const operation = await first.start("source", "turn", "replacement");
  expect(operation.sessionId).toBe("child");
  expect(operation.stage).toBe("input_unconfirmed");
  expect(get(first.pending)).toHaveLength(1);
  const restored = createHistoryForks(device);
  api.sendMessage.mockResolvedValue({ status: "queued", message_id: "queued" });
  const recovered = await restored.resume(get(restored.pending)[0]);
  expect(recovered.stage).toBe("input_submitted");
  expect(api.forkSession).toHaveBeenCalledTimes(1);
  expect(api.sendMessage.mock.calls).toEqual([
    [
      "child",
      {
        input: "replacement",
        mode: "queue",
        metadata: { source: "dashboard_history_fork" },
      },
      `fork-${operation.operationId}`,
    ],
    [
      "child",
      {
        input: "replacement",
        mode: "queue",
        metadata: { source: "dashboard_history_fork" },
      },
      `fork-${operation.operationId}`,
    ],
  ]);
  expect(get(restored.pending)).toEqual([]);
});

test.each(["session_pending", "fork_unconfirmed"])(
  "%s is recoverable without changing the fork operation identity",
  async (stage) => {
    api.forkSession.mockResolvedValueOnce({ stage, session_id: null, error: "pending" });
    const store = createHistoryForks(device);
    const operation = await store.start("source", "turn", "replacement");
    expect(api.sendMessage).not.toHaveBeenCalled();
    api.forkSession.mockResolvedValue({
      stage: "session_created",
      session_id: "child",
      error: null,
    });
    api.sendMessage.mockResolvedValue({ status: "queued", message_id: "queued" });
    await store.resume(operation);
    expect(api.forkSession.mock.calls).toEqual([
      ["source", "turn", operation.operationId],
      ["source", "turn", operation.operationId],
    ]);
    expect(get(store.pending)).toEqual([]);
  },
);

test("native rejection keeps the edit and never submits to the source", async () => {
  api.forkSession.mockResolvedValue({ stage: "fork_failed", session_id: null, error: "rejected" });
  const store = createHistoryForks(device);
  await store.start("source", "turn", "replacement");
  expect(api.sendMessage).not.toHaveBeenCalled();
  expect(get(store.pending)[0]).toMatchObject({
    stage: "fork_failed",
    input: "replacement",
    sessionId: null,
  });
});
