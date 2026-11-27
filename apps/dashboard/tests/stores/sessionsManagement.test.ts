import { beforeEach, describe, expect, test, vi } from "vitest";
import type { QueuedMessageView, SessionView } from "../../src/api/types";

const session: SessionView = {
  session_id: "session-current",
  client_type: "pi",
  title: "Current session",
  handle: null,
  role: null,
  description: null,
  workspace: null,
  workspace_id: null,
  workspace_ref: null,
  state: "idle",
  current_turn_id: null,
  state_version: 1,
  metadata: {},
  capabilities: {},
  created_at: "2026-06-22T00:00:00.000Z",
  updated_at: "2026-06-22T00:00:00.000Z",
};

const queuedMessage: QueuedMessageView = {
  message_id: "message-queued",
  session_id: session.session_id,
  input: "Follow up",
  metadata: { source: "dashboard_chat" },
  created_at: "2026-06-22T00:01:00.000Z",
};

const api = vi.hoisted(() => ({
  createSession: vi.fn(),
  getSessionOverview: vi.fn(),
  getSession: vi.fn(),
  listTurns: vi.fn(),
  listQueue: vi.fn(),
  listEvents: vi.fn(),
  updateSession: vi.fn(),
  sendMessage: vi.fn(),
  cancelQueuedMessage: vi.fn(),
  interruptSession: vi.fn(),
  resumeSession: vi.fn(),
  terminateSession: vi.fn(),
  pinSession: vi.fn(),
  unpinSession: vi.fn(),
  archiveSession: vi.fn(),
}));

vi.mock("../../src/api/client", async (importOriginal) => {
  const original = await importOriginal<typeof import("../../src/api/client")>();
  return {
    ...original,
    createApi: (device: never) => ({ ...original.createApi(device), ...api }),
  };
});

describe("sessions store message actions", () => {
  beforeEach(() => {
    vi.resetModules();
    Object.values(api).forEach((mock) => mock.mockReset());
    api.getSessionOverview.mockResolvedValue({
      groups: {
        pinned: { sessions: [] },
        active: { sessions: [] },
        list: { sessions: [], next_cursor: null },
      },
    });
    api.getSession.mockResolvedValue(session);
    api.listTurns.mockResolvedValue([]);
    api.listQueue.mockResolvedValue([]);
    api.listEvents.mockResolvedValue([]);
  });

  test("sends a message with its selected mode", async () => {
    api.sendMessage.mockResolvedValue({ status: "queued", message_id: queuedMessage.message_id });
    const { sendMessage } = (await import("../deviceFixture")).testDevice.sessions;

    await expect(
      sendMessage(session.session_id, {
        input: queuedMessage.input,
        mode: "queue",
        metadata: queuedMessage.metadata,
      }),
    ).resolves.toEqual({ status: "queued", message_id: queuedMessage.message_id });
    expect(api.sendMessage).toHaveBeenCalledWith(session.session_id, {
      input: queuedMessage.input,
      mode: "queue",
      metadata: queuedMessage.metadata,
    });
  });

  test("cancelling removes the message from the cached queue", async () => {
    api.cancelQueuedMessage.mockResolvedValue(queuedMessage);
    const device = (await import("../deviceFixture")).testDevice;
    device.sessionQueries.setSessionDetail({
      session,
      turns: [],
      queuedMessages: [queuedMessage],
      events: [],
    });

    await device.sessions.cancelQueuedMessage(session.session_id, queuedMessage.message_id);

    expect(
      device.queryClient.getQueryData(device.sessionQueries.sessionKeys.queue(session.session_id)),
    ).toEqual([]);
  });
});
