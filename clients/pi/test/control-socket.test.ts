import { mkdir, rmdir } from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { dirname, join } from "node:path";
import { onTestFinished, expect, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";
import { connectPi } from "../src/control-socket.js";
import {
  controlSocketPath,
  formatSocketPath,
  AttachmentRejected,
  MAX_RPC_FRAME_BYTES,
  RpcError,
} from "../shared/control-socket.js";
import { LiveOutputPublisher } from "../src/live-output.js";
import { EventReporter } from "../src/reporter.js";
import { buildTurnStartedEvent } from "../src/events.js";
import { tempDir } from "./temp-dir.js";

// Pi plugins, Claude hosts and ACP relays keep computing this path with their startup
// version across pontiad upgrades. Moving it in 9d550ec6 from $PONTIA_HOME/state/pi/rpc.sock
// left running agents retrying the old path, disconnected and impossible to close from Pontia.
// /tmp fits the ~104–108 byte Unix socket limit even with a long PONTIA_HOME.
// Changing this path requires project owner approval.
test("the control socket path is stable across upgrades", () => {
  expect(formatSocketPath(1001n, 0xabcdn, 0x123456789abcdefn)).toBe(
    "/tmp/pontia-control-1001-abcd-123456789abcdef/rpc.sock",
  );
});

async function server(handler: (socket: Socket, message: any) => void) {
  const root = await tempDir("pr-");
  await mkdir(dirname(controlSocketPath(root)), { recursive: true });
  const sockets = new Set<Socket>();
  const listener = createServer((socket) => {
    socket.setEncoding("utf8");
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
    socket.on("error", () => {});
    let pending = "";
    socket.on("data", (chunk) => {
      pending += chunk.toString();
      for (;;) {
        const index = pending.indexOf("\n");
        if (index < 0) break;
        const line = pending.slice(0, index);
        pending = pending.slice(index + 1);
        handler(socket, JSON.parse(line));
      }
    });
  });
  await new Promise<void>((resolve) => listener.listen(controlSocketPath(root), resolve));
  onTestFinished(async () => {
    for (const socket of sockets) socket.destroy();
    await new Promise<void>((resolve) => listener.close(() => resolve()));
    await rmdir(dirname(controlSocketPath(root)));
  });
  return root;
}

function reply(socket: Socket, id: unknown, result: unknown) {
  socket.write(`${JSON.stringify({ jsonrpc: "2.0", id, result })}\n`);
}

test("steer forwards its target Turn and rejects a missing target", async () => {
  const responses: any[] = [];
  const root = await server((socket, message) => {
    if (!message.method) {
      responses.push(message);
      return;
    }
    for (const [id, params] of [
      [
        "valid",
        { input: "Adjust the task", inbox_message_id: "msg_steer", turn_id: "turn_active" },
      ],
      ["missing", { input: "Adjust the task" }],
    ]) {
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id, method: "steer", params })}\n`);
    }
    reply(socket, message.id, {});
  });
  const submit = vi.fn();
  const client = await connectPi(root, () => {}, submit);
  onTestFinished(() => client.close());
  await client.request("begin", {});
  await vi.waitFor(() => expect(responses).toHaveLength(2));
  expect(submit).toHaveBeenCalledExactlyOnceWith({
    input: "Adjust the task",
    inboxMessageId: "msg_steer",
    steerTargetTurnId: "turn_active",
  });
  expect(responses.find((response) => response.id === "missing").error.code).toBe(-32602);
});

test("compaction control acknowledges native initiation and rejects malformed operations", async () => {
  const responses: any[] = [];
  const root = await server((socket, message) => {
    if (!message.method) {
      responses.push(message);
      return;
    }
    for (const [id, params] of [
      ["valid", { operation_id: "compact-api" }],
      ["empty", { operation_id: "" }],
      ["extra", { operation_id: "other", input: "/compact" }],
    ]) {
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id, method: "compact", params })}\n`);
    }
    reply(socket, message.id, {});
  });
  const compact = vi.fn();
  const client = await connectPi(
    root,
    () => {},
    () => {},
    undefined,
    undefined,
    undefined,
    undefined,
    { compact },
  );
  onTestFinished(() => client.close());
  await client.request("begin", {});
  await vi.waitFor(() => expect(responses).toHaveLength(3));
  expect(compact).toHaveBeenCalledExactlyOnceWith("compact-api");
  expect(responses.find((r) => r.id === "valid").result).toEqual({ accepted: true });
  expect(responses.filter((r) => r.error).map((r) => r.error.code)).toEqual([-32602, -32602]);
});

test("reasoning effort control accepts native Pi levels and rejects malformed requests", async () => {
  const responses: any[] = [];
  const root = await server((socket, message) => {
    if (!message.method) {
      responses.push(message);
      return;
    }
    for (const [id, params] of [
      ["valid", { reasoning_effort: "xhigh" }],
      ["unknown", { reasoning_effort: "extreme" }],
      ["extra", { reasoning_effort: "high", persist: true }],
    ]) {
      socket.write(
        `${JSON.stringify({ jsonrpc: "2.0", id, method: "set_reasoning_effort", params })}\n`,
      );
    }
    reply(socket, message.id, {});
  });
  const setReasoningEffort = vi.fn();
  const client = await connectPi(
    root,
    () => {},
    () => {},
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    {},
    { setReasoningEffort },
  );
  onTestFinished(() => client.close());
  await client.request("begin", {});
  await vi.waitFor(() => expect(responses).toHaveLength(3));
  expect(setReasoningEffort).toHaveBeenCalledExactlyOnceWith("xhigh");
  expect(responses.find((response) => response.id === "valid").result).toEqual({ accepted: true });
  expect(
    responses.filter((response) => response.error).map((response) => response.error.code),
  ).toEqual([-32602, -32602]);
});

test("branch replay can resolve over the same socket and rejects malformed replay identifiers", async () => {
  const replies: any[] = [];
  const root = await server((socket, message) => {
    if (!message.method) {
      replies.push(message);
      return;
    }
    if (message.method === "begin") {
      for (const [id, replay_id] of [
        ["valid", "replay_valid"],
        ["invalid", "replay_one extra"],
      ]) {
        socket.write(
          `${JSON.stringify({ jsonrpc: "2.0", id, method: "branch.replay", params: { replay_id } })}\n`,
        );
      }
    }
    reply(
      socket,
      message.id,
      message.method === "branch.resolve" ? { target_entry_id: "native-user" } : {},
    );
  });
  let resolved: Promise<unknown> | undefined;
  const replay = vi.fn((replayId: string) => {
    resolved = client.request("branch.resolve", { replay_id: replayId });
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
    undefined,
    replay,
  );
  onTestFinished(() => client.close());
  await client.request("begin", {});
  await vi.waitFor(() => expect(replies).toHaveLength(2));
  await expect(resolved).resolves.toEqual({ target_entry_id: "native-user" });
  expect(replay).toHaveBeenCalledExactlyOnceWith("replay_valid");
  expect(replies).toContainEqual({ jsonrpc: "2.0", id: "valid", result: { accepted: true } });
  expect(replies.find((response) => response.id === "invalid").error.code).toBe(-32602);
});

test("dispatches daemon requests while a registration request is awaiting its reply", async () => {
  let registrationId: unknown;
  const root = await server((socket, message) => {
    if (message.method === "runtime.register") {
      registrationId = message.id;
      socket.write(
        `${JSON.stringify({ jsonrpc: "2.0", id: "daemon:1", method: "submit", params: { input: "你好", inbox_message_id: "msg_1" } })}\n`,
      );
    } else {
      expect(message.id).toBe("daemon:1");
      expect(message.result).toEqual({ accepted: true });
      reply(socket, registrationId, { session_id: "s", runtime_id: "r" });
    }
  });
  const submit = vi.fn();
  const client = await connectPi(root, () => {}, submit);
  onTestFinished(() => client.close());
  await expect(client.request("runtime.register", {})).resolves.toEqual({
    session_id: "s",
    runtime_id: "r",
  });
  expect(submit).toHaveBeenCalledWith({ input: "你好", inboxMessageId: "msg_1" });
});

test("reconnect attaches the confirmed identity without repeating registration", async () => {
  const methods: string[] = [];
  const root = await server((socket, message) => {
    methods.push(message.method);
    if (message.method === "runtime.register") reply(socket, message.id, {});
    else if (message.method === "break") socket.destroy();
    else {
      expect(message.method).toBe("runtime.attach");
      expect(message.params).toMatchObject({
        session_id: "s",
        runtime_id: "r",
        client_session_key: "native",
      });
      reply(socket, message.id, { session_id: "s", runtime_id: "r" });
    }
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
  );
  onTestFinished(() => client.close());
  await client.request("runtime.register", {});
  client.registered({ sessionId: "s", runtimeId: "r", clientSessionKey: "native" });
  await expect(client.request("break", {})).rejects.toThrow();
  await vi.waitFor(() => expect(methods).toEqual(["runtime.register", "break", "runtime.attach"]));
});

test.each(["wrong-id", "both", "invalid-json", "oversized"])(
  "rejects %s frames and fails pending calls",
  async (kind) => {
    const root = await server((socket, message) => {
      const frame =
        kind === "wrong-id"
          ? JSON.stringify({ jsonrpc: "2.0", id: "wrong", result: {} })
          : kind === "both"
            ? JSON.stringify({
                jsonrpc: "2.0",
                id: message.id,
                result: {},
                error: { code: 1, message: "bad" },
              })
            : kind === "oversized"
              ? "x".repeat(MAX_RPC_FRAME_BYTES + 1)
              : "not-json";
      socket.write(`${frame}\n`);
    });
    const client = await connectPi(
      root,
      () => {},
      () => {},
    );
    onTestFinished(() => client.close());
    await expect(client.request("runtime.register", {})).rejects.toThrow();
  },
);

test("correlates replies that arrive in reverse order", async () => {
  const pending: any[] = [];
  const root = await server((socket, message) => {
    pending.push(message);
    if (pending.length === 2)
      for (const request of pending.reverse()) reply(socket, request.id, request.method);
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
  );
  onTestFinished(() => client.close());
  await expect(
    Promise.all([client.request("one", {}), client.request("two", {})]),
  ).resolves.toEqual(["one", "two"]);
});

test("lost turn.started acknowledgements retry the same report over a fresh connection", async () => {
  const methods: string[] = [];
  let reports = 0;
  const root = await server((socket, message) => {
    if (!message.method) {
      expect(message.id).toBe("daemon:submit");
      expect(message.result).toEqual({ accepted: true });
      socket.destroy();
      return;
    }
    methods.push(message.method);
    if (message.method === "event.report") {
      reports++;
      expect(message.params.event.report_key).toBe("entry_1");
      if (reports === 1) {
        socket.write(
          `${JSON.stringify({ jsonrpc: "2.0", id: "daemon:submit", method: "submit", params: { input: "next" } })}\n`,
        );
      } else {
        reply(socket, message.id, {
          accepted: true,
          duplicate: true,
          event_id: "evt_1",
          turn_id: "turn_1",
        });
      }
    } else if (message.method === "runtime.attach") {
      setTimeout(() => reply(socket, message.id, { session_id: "s", runtime_id: "r" }), 400);
    }
  });
  const submit = vi.fn();
  const client = await connectPi(root, () => {}, submit);
  onTestFinished(() => client.close());
  client.registered({ sessionId: "s", runtimeId: "r", clientSessionKey: "native" });
  const reporter = new EventReporter({ connection: client, logFile: join(root, "hook.log") });
  const context = { sessionId: "s", runtimeId: "r", clientType: "pi" as const };
  expect(await reporter.report(context, buildTurnStartedEvent(context, "entry_1"))).toEqual({
    accepted: true,
    eventId: "evt_1",
    turnId: "turn_1",
  });
  expect(methods.filter((method) => method !== "runtime.attach")).toEqual([
    "event.report",
    "event.report",
  ]);
  expect(submit).toHaveBeenCalledWith({ input: "next", inboxMessageId: undefined });
});

test("reports large facts without treating normal socket buffering as failure", async () => {
  const data = "界".repeat(40_000);
  const root = await server((socket, message) => {
    expect(message.params.event.data.input_summary).toBe(data);
    reply(socket, message.id, { accepted: true, turn_id: "turn_1" });
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
  );
  onTestFinished(() => client.close());
  await expect(
    client.request("event.report", { event: { data: { input_summary: data } } }),
  ).resolves.toMatchObject({ accepted: true });
});

test("an incompatible reconnect permanently rejects pending and subsequent requests", async () => {
  const attempts = vi.fn();
  const root = await server((socket, message) => {
    if (message.method === "break") return socket.destroy();
    attempts(message.params.version);
    socket.end(
      `${JSON.stringify({ jsonrpc: "2.0", id: message.id, error: { code: -32012, message: "Restart this Session." } })}\n`,
    );
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
  );
  onTestFinished(() => client.close());
  client.registered({ sessionId: "s", runtimeId: "r" });
  await expect(client.request("break", {})).rejects.toThrow();
  await expect(client.request("event.report", {})).rejects.toBeInstanceOf(AttachmentRejected);
  await expect(client.request("session.get", {})).rejects.toBeInstanceOf(AttachmentRejected);
  await new Promise((resolve) => setTimeout(resolve, 600));
  expect(attempts).toHaveBeenCalledExactlyOnceWith(8);
});

test("new event reports wait for reconnect attachment before they are sent", async () => {
  let attached = false;
  const methods: string[] = [];
  const root = await server((socket, message) => {
    methods.push(message.method);
    if (message.method === "break") {
      socket.destroy();
      return;
    }
    if (message.method === "runtime.attach") {
      setTimeout(() => {
        attached = true;
        reply(socket, message.id, { session_id: "s", runtime_id: "r" });
      }, 100);
      return;
    }
    expect(attached).toBe(true);
    reply(socket, message.id, { accepted: true });
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
  );
  onTestFinished(() => client.close());
  client.registered({ sessionId: "s", runtimeId: "r", clientSessionKey: "native" });
  await expect(client.request("break", {})).rejects.toThrow();
  await vi.waitFor(() => expect(methods).toContain("runtime.attach"));
  await expect(client.request("event.report", {})).resolves.toEqual({ accepted: true });
  expect(methods).toEqual(["break", "runtime.attach", "event.report"]);
});

test("live output recovers a lost acknowledgement after attaching and keeps control responsive", async () => {
  const methods: string[] = [];
  const publications: any[] = [];
  let pingAnswered = false;
  const root = await server((socket, message) => {
    if (!message.method) {
      expect(message).toEqual({ jsonrpc: "2.0", id: "daemon:ping", result: { pong: true } });
      pingAnswered = true;
      return;
    }
    methods.push(message.method);
    if (message.method === "runtime.attach") {
      reply(socket, message.id, { session_id: "s", runtime_id: "r" });
      return;
    }
    expect(message.method).toBe("liveOutput.publish");
    publications.push(message.params);
    if (publications.length === 1) {
      socket.destroy();
      return;
    }
    if (publications.length === 2) {
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: "daemon:ping", method: "ping" })}\n`);
    }
    reply(socket, message.id, {
      accepted: true,
      accepted_sequence: message.params.sequence,
      duplicate: false,
      resync_required: false,
    });
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
  );
  onTestFinished(() => client.close());
  client.registered({ sessionId: "s", runtimeId: "r", clientSessionKey: "native" });
  const publisher = new LiveOutputPublisher(
    {
      sessionId: "s",
      runtimeId: "r",
      turnId: "t",
      clientType: "pi",
    },
    { connection: client },
  );
  onTestFinished(() => publisher.close());
  publisher.appendText("hello");
  await vi.waitFor(() => expect(publications).toHaveLength(1));
  publisher.appendText(" world");
  await vi.waitFor(() => expect(publications).toHaveLength(2), { timeout: 2_000 });
  await vi.waitFor(() => expect(pingAnswered).toBe(true));
  await publisher.close();
  expect(methods).toEqual([
    "liveOutput.publish",
    "runtime.attach",
    "liveOutput.publish",
    "liveOutput.publish",
  ]);
  expect(publications).toEqual([
    expect.objectContaining({
      type: "snapshot",
      sequence: 1,
      items: [{ kind: "assistant_text", item_id: "text_1", text: "hello" }],
    }),
    expect.objectContaining({
      type: "snapshot",
      sequence: 2,
      items: [{ kind: "assistant_text", item_id: "text_1", text: "hello world" }],
    }),
    expect.objectContaining({ type: "stream_closed", sequence: 3 }),
  ]);
});

test("oversized live output stops retrying without closing the shared control connection", async () => {
  const methods: string[] = [];
  const root = await server((socket, message) => {
    methods.push(message.method);
    reply(socket, message.id, {});
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
  );
  onTestFinished(() => client.close());
  const request = vi.fn(client.request);
  const publisher = new LiveOutputPublisher(
    {
      sessionId: "s",
      runtimeId: "r",
      turnId: "t",
      clientType: "pi",
    },
    { connection: { request } },
  );
  onTestFinished(() => publisher.close());
  publisher.appendText("x".repeat(MAX_RPC_FRAME_BYTES));
  await vi.waitFor(() => expect(request).toHaveBeenCalledOnce());
  publisher.appendText("more");
  await publisher.close();
  expect(request).toHaveBeenCalledOnce();
  await expect(client.request("ping", {})).resolves.toEqual({});
  expect(methods).toEqual(["ping"]);
});

test.each(["interrupt", "shutdown"] as const)(
  "%s acknowledges native requests and preserves rejection and uncertainty",
  async (method) => {
    const responses: any[] = [];
    const root = await server((socket, message) => {
      if (!message.method) {
        responses.push(message);
        return;
      }
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: "control", method })}\n`);
      reply(socket, message.id, {});
    });
    const lifecycle = { interrupt: vi.fn(), shutdown: vi.fn(), shutdownIfIdle: vi.fn() };
    const callback = lifecycle[method];
    const client = await connectPi(
      root,
      () => {},
      () => {},
      undefined,
      undefined,
      lifecycle,
    );
    onTestFinished(() => client.close());
    await client.request("begin", {});
    await vi.waitFor(() => expect(responses).toHaveLength(1));
    expect(callback).toHaveBeenCalledOnce();
    expect(responses[0].result).toEqual({ accepted: true });
    callback.mockImplementationOnce(() => {
      throw new Error("Session is no longer current");
    });
    await client.request("begin", {});
    await vi.waitFor(() => expect(responses).toHaveLength(2));
    expect(responses[1].error.code).toBe(-32006);
    callback.mockImplementationOnce(() => {
      throw new RpcError(-32007, "Native result is unknown");
    });
    await client.request("begin", {});
    await vi.waitFor(() => expect(responses).toHaveLength(3));
    expect(responses[2].error.code).toBe(-32007);
  },
);

test("serves large image history while branch.resolve awaits a reverse response", async () => {
  let branchId: unknown;
  const root = await server((socket, message) => {
    if (message.method === "branch.resolve") {
      branchId = message.id;
      socket.write(
        `${JSON.stringify({ jsonrpc: "2.0", id: "history", method: "history.read", params: {} })}\n`,
      );
    } else if (message.id === "history") {
      expect(message.result.entries[0].id).toBe("user");
      expect(message.result.entries[1]).toMatchObject({
        id: "image",
        parentId: "user",
        message: {
          role: "toolResult",
          toolCallId: "call-1",
          content: [{ type: "image", mimeType: "image/png" }],
        },
      });
      expect(message.result.entries[1].message.content[0]).not.toHaveProperty("data");
      expect(Buffer.byteLength(JSON.stringify(message))).toBeLessThan(MAX_RPC_FRAME_BYTES);
      reply(socket, branchId, { target_entry_id: "user" });
    } else reply(socket, message.id, {});
  });
  const client = await connectPi(
    root,
    () => {},
    () => {},
    undefined,
    undefined,
    undefined,
    () => ({
      generation: 1,
      sessionManager: {
        getSessionId: () => "native",
        getLeafId: () => "image",
        getEntries: () => [
          {
            id: "user",
            parentId: null,
            type: "message",
            timestamp: "2026-10-11T00:00:00Z",
            message: { role: "user", content: "question" },
          },
          {
            id: "image",
            parentId: "user",
            type: "message",
            timestamp: "2026-10-11T00:00:01Z",
            message: {
              role: "toolResult",
              toolCallId: "call-1",
              toolName: "read",
              content: [{ type: "image", mimeType: "image/png", data: "a".repeat(2_546_580) }],
              isError: false,
            },
          },
        ],
      },
    }),
  );
  onTestFinished(() => client.close());
  await expect(client.request("branch.resolve", {})).resolves.toEqual({ target_entry_id: "user" });
  await expect(client.request("ping", {})).resolves.toEqual({});
});

test("conditional shutdown reports native rejection without terminating the channel", async () => {
  const responses: any[] = [];
  const root = await server((socket, message) => {
    if (!message.method) {
      responses.push(message);
      return;
    }
    socket.write(
      `${JSON.stringify({ jsonrpc: "2.0", id: "conditional", method: "shutdown_if_idle" })}\n`,
    );
    reply(socket, message.id, {});
  });
  let idle = false;
  const client = await connectPi(
    root,
    () => {},
    () => {},
    undefined,
    undefined,
    {
      interrupt: vi.fn(),
      shutdown: vi.fn(),
      shutdownIfIdle: () => idle,
    },
  );
  onTestFinished(() => client.close());
  await client.request("begin", {});
  await vi.waitFor(() => expect(responses).toHaveLength(1));
  expect(responses[0].result).toEqual({ accepted: false });
  idle = true;
  await client.request("begin", {});
  await vi.waitFor(() => expect(responses).toHaveLength(2));
  expect(responses[1].result).toEqual({ accepted: true });
});

test("extension reload retains the socket and FIFO while a ready receipt is outstanding", async () => {
  const sockets = new Set<Socket>();
  const facts: string[] = [];
  let acknowledge: (() => void) | undefined;
  const root = await server((socket, { method, id, params }) => {
    sockets.add(socket);
    if (method === "session.context")
      return reply(socket, id, {
        session_context: {
          session_id: "sess_reload",
          runtime_id: "rt_reload",
          client_type: "pi",
          session_state: "starting",
        },
      });
    if (method === "workspaces.list")
      return reply(socket, id, {
        workspaces: [{ canonical_path: root, state: "active" }],
      });
    if (method === "runtime.register")
      return reply(socket, id, {
        session: { session_id: "sess_reload" },
        runtime: { runtime_id: "rt_reload" },
      });
    if (method === "event.report") {
      facts.push(params.event.type);
      if (params.event.type === "session.ready") {
        acknowledge = () => reply(socket, id, { accepted: true });
        return;
      }
      return reply(socket, id, { accepted: true, turn_id: "turn_reloaded" });
    }
    reply(socket, id, {});
  });
  const handlers: Record<string, (event: any, ctx?: any) => Promise<void>> = {};
  const install = () =>
    createPontiaPiExtension(
      {
        on(name: string, handler: any) {
          handlers[name] = handler;
        },
        registerCommand() {},
      } as any,
      { env: { PONTIA_HOME: root } },
    );
  const context = {
    mode: "tui",
    sessionManager: {
      getSessionId: () => "native",
      getSessionFile: () => join(root, "session.jsonl"),
      getCwd: () => root,
      getLeafId: () => "native-entry",
    },
  };
  install();
  await handlers.session_start({ reason: "startup" }, context);
  await vi.waitFor(() => expect(acknowledge).toBeDefined());
  await handlers.session_shutdown({ reason: "reload" }, context);
  install();
  await handlers.session_start({ reason: "reload" }, context);
  await handlers.agent_start({}, context);
  await handlers.agent_end({ messages: [] }, context);
  expect(facts).toEqual(["session.ready"]);
  acknowledge!();
  await vi.waitFor(() =>
    expect(facts).toEqual(["session.ready", "turn.started", "turn.completed"]),
  );
  await handlers.session_shutdown({ reason: "quit" }, context);
  await vi.waitFor(() => expect(facts.at(-1)).toBe("session.exited"));
  expect(sockets.size).toBe(1);
});
