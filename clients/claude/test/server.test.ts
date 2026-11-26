import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtempDisposable, mkdir, rmdir } from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test, type TestContext } from "node:test";
import { bootstrap, serve } from "../src/server.js";
import { controlSocketPath } from "../shared/control-socket.js";
import { assistant, first, init, result, sdk, second } from "./fixture.js";

type Frame = Record<string, any>;
async function setup(t: TestContext, rejectKind?: string, transient = false) {
  const sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-host-test-"));
  const path = controlSocketPath(sandbox.path);
  await mkdir(dirname(path), { mode: 0o700 });
  const fixture = sdk();
  let socket: Socket;
  let count = 0;
  let terminal = false;
  let nextId = 1;
  const facts: Frame[] = [];
  const replies = new Map<number, (frame: Frame) => void>();
  const listener = createServer((client) => {
    socket = client;
    client.setEncoding("utf8");
    let buffer = "";
    client.on("data", (chunk) => {
      buffer += chunk;
      while (buffer.includes("\n")) {
        const end = buffer.indexOf("\n");
        const frame = JSON.parse(buffer.slice(0, end));
        buffer = buffer.slice(end + 1);
        if (frame.method === "runtime.attach") {
          count++;
          client.write(
            JSON.stringify({
              jsonrpc: "2.0",
              id: frame.id,
              ...(terminal
                ? { error: { code: -32009, message: "Session exited" } }
                : {
                    result: {
                      session_id: frame.params.session_id,
                      runtime_id: frame.params.runtime_id,
                    },
                  }),
            }) + "\n",
          );
        } else if (frame.method === "host.event") {
          facts.push(frame.params);
          if (transient && facts.filter((f) => f.event.kind === rejectKind).length > 1)
            rejectKind = undefined;
          client.write(
            JSON.stringify({
              jsonrpc: "2.0",
              id: frame.id,
              ...(frame.params.event.kind === rejectKind
                ? { error: { code: transient ? -32603 : -32009, message: "rejected fact" } }
                : { result: { accepted: true } }),
            }) + "\n",
          );
        } else {
          const resolve = replies.get(frame.id);
          replies.delete(frame.id);
          resolve?.(frame.result);
        }
      }
    });
  });
  listener.listen(path);
  await once(listener, "listening");
  const config = {
    session_id: "session",
    runtime_id: "runtime",
    cwd: sandbox.path,
    mode: "default" as const,
    pontia_home: sandbox.path,
  };
  const host = await serve(config, fixture.factory);
  async function wait(predicate: () => boolean) {
    for (let attempt = 0; attempt < 400; attempt++) {
      if (predicate()) return;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    throw new Error("observation timed out");
  }
  function call(method: string, params: object): Promise<Frame> {
    const id = nextId++;
    return new Promise((resolve) => {
      replies.set(id, resolve);
      socket.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  }
  async function reconnect(whileOffline?: () => Promise<void>) {
    const previous = count;
    socket.destroy();
    await new Promise<void>((resolve) => listener.close(() => resolve()));
    await whileOffline?.();
    listener.listen(path);
    await once(listener, "listening");
    await wait(() => count > previous);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  t.after(async () => {
    fixture.messages.end();
    await host.close();
    socket?.destroy();
    await new Promise<void>((resolve) => listener.close(() => resolve()));
    await rmdir(dirname(path));
    await sandbox.remove();
  });
  return {
    fixture,
    facts,
    wait,
    reconnect,
    endSession: () => {
      terminal = true;
    },
    command: (params: { method: string; text?: string; [key: string]: unknown }) =>
      call(
        "host.command",
        params.method === "input"
          ? { ...params, input: { summary: params.text, inbox_message_id: null } }
          : params,
      ),
    status: () => call("host.status", {}),
  };
}

test("bootstrap accepts the three session modes and rejects unknown modes", () => {
  const config = {
    session_id: "session",
    runtime_id: "runtime",
    pontia_home: "/unused",
    cwd: "/unused",
  };
  for (const mode of ["default", "yolo", "restricted"]) {
    assert.equal(bootstrap({ ...config, launch: { cwd: "/unused", mode } }).mode, mode);
  }
  assert.throws(() => bootstrap(config));
  assert.throws(() => bootstrap({ ...config, launch: { cwd: "/unused", mode: "other" } }));
});

test(
  "dial-in control accepts input and rejects duplicate native input without executing it again",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t);
    assert.equal(
      (await h.command({ method: "input", input_id: first, text: "task" })).status,
      "accepted",
    );
    assert.equal((await h.fixture.next()).value.uuid, first);
    assert.equal(
      (await h.command({ method: "input", input_id: first, text: "task" })).status,
      "rejected",
    );
    assert.equal(h.fixture.calls, 1);
  },
);

test("reasoning effort control reports the SDK-confirmed value", { timeout: 5000 }, async (t) => {
  const h = await setup(t);
  await h.command({ method: "input", input_id: first, text: "task" });
  h.fixture.messages.push(init());
  await h.wait(() => h.facts.some((fact) => fact.event.kind === "reasoning_effort"));
  h.fixture.onApplyFlagSettings(async () => h.fixture.setEffort("low"));
  assert.equal(
    (await h.command({ method: "set_reasoning_effort", reasoning_effort: "max" })).status,
    "accepted",
  );
  await h.wait(
    () =>
      h.facts.filter((fact) => fact.event.kind === "reasoning_effort").at(-1)?.event
        .reasoning_effort === "low",
  );
  assert.deepEqual(h.fixture.appliedEfforts, ["max"]);
  assert.equal(
    (await h.command({ method: "set_reasoning_effort", reasoning_effort: "minimal" })).status,
    "rejected",
  );
});

test(
  "an executing SDK survives reconnect and completes on the replacement connection",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t);
    await h.command({ method: "input", input_id: first, text: "task" });
    await h.fixture.next();
    h.fixture.messages.push(init());
    h.fixture.messages.push(assistant());
    await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
    const before = await h.status();
    await h.reconnect();
    const after = await h.status();
    assert.equal(after.host_id, before.host_id);
    assert.equal(after.input_id, first);
    assert.equal(after.closed, false);
    h.fixture.messages.push(result());
    await h.wait(() => h.facts.some((f) => f.event.kind === "finished"));
    assert.equal(
      (await h.command({ method: "input", input_id: second, text: "next" })).status,
      "accepted",
    );
    assert.equal((await h.fixture.next()).value.uuid, second);
    assert.equal(h.fixture.calls, 1);
  },
);

test(
  "a Turn finishing offline is reported once after reconnection",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t);
    await h.command({ method: "input", input_id: first, text: "task" });
    h.fixture.messages.push(init());
    h.fixture.messages.push(assistant());
    await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
    await h.reconnect(async () => {
      h.fixture.messages.push(result());
      await new Promise((resolve) => setTimeout(resolve, 100));
    });
    await h.wait(() => h.facts.some((f) => f.event.kind === "finished"));
    assert.equal(h.facts.filter((f) => f.event.kind === "finished").length, 1);
    assert.equal(
      (await h.command({ method: "input", input_id: second, text: "next" })).status,
      "accepted",
    );
  },
);

test(
  "interrupt remains usable after reconnect without claiming completion",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t);
    await h.command({ method: "input", input_id: first, text: "task" });
    h.fixture.messages.push(init());
    h.fixture.messages.push(assistant());
    await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
    await h.reconnect();
    assert.equal((await h.command({ method: "interrupt", input_id: first })).status, "accepted");
    assert.equal(h.fixture.interrupts, 1);
    assert.equal(
      h.facts.some((f) => f.event.kind === "finished"),
      false,
    );
  },
);

test("close confirms SDK closure on the reattached connection", { timeout: 5000 }, async (t) => {
  const h = await setup(t);
  await h.command({ method: "input", input_id: first, text: "task" });
  h.fixture.messages.push(init());
  h.fixture.messages.push(assistant());
  await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
  await h.reconnect();
  assert.equal((await h.command({ method: "close" })).status, "accepted");
  assert.equal(
    h.facts.some((f) => f.event.kind === "closed"),
    true,
  );
});

test(
  "approvals requested after reattachment reach the waiting SDK callback",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t);
    await h.command({ method: "input", input_id: first, text: "task" });
    h.fixture.messages.push(init());
    h.fixture.messages.push(assistant());
    await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
    await h.reconnect();
    const response = h.fixture.options!.canUseTool!(
      "Write",
      { file_path: "test.txt" },
      { signal: new AbortController().signal, toolUseID: "tool", requestId: "permission" },
    );
    await h.wait(() => h.facts.some((f) => f.event.kind === "interaction_requested"));
    const permission = h.facts.find((f) => f.event.kind === "interaction_requested")!.event;
    assert.equal(
      (
        await h.command({
          method: "interaction",
          interaction_key: permission.interaction_key,
          option: "allow",
        })
      ).status,
      "accepted",
    );
    assert.equal((await response)?.behavior, "allow");
  },
);

test(
  "reconnect before first input leaves the SDK launch available",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t);
    await h.reconnect();
    assert.equal(
      (await h.command({ method: "input", input_id: first, text: "task" })).status,
      "accepted",
    );
    assert.equal(h.fixture.calls, 1);
  },
);

test("an outstanding interrupt does not prevent closing the SDK", { timeout: 5000 }, async (t) => {
  const h = await setup(t);
  await h.command({ method: "input", input_id: first, text: "task" });
  h.fixture.messages.push(init());
  h.fixture.messages.push(assistant());
  await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
  let release!: () => void;
  h.fixture.onInterrupt(
    () =>
      new Promise<void>((resolve) => {
        release = resolve;
      }),
  );
  void h.command({ method: "interrupt", input_id: first });
  await h.wait(() => h.fixture.interrupts === 1);
  try {
    assert.equal((await h.command({ method: "close" })).status, "accepted");
    assert.ok(h.facts.some((f) => f.event.kind === "closed"));
  } finally {
    release();
  }
});

test(
  "a report conflict is final and later observations retain their own keys",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t, "model");
    await h.command({ method: "input", input_id: first, text: "task" });
    h.fixture.messages.push(init());
    h.fixture.messages.push(assistant());
    await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
    await h.reconnect();
    h.fixture.messages.push(result());
    await h.wait(() => h.facts.some((f) => f.event.kind === "finished"));
    assert.equal(h.facts.filter((f) => f.event.kind === "model").length, 1);
    for (const kind of ["ready", "model", "started", "finished"]) {
      const fact = h.facts.find((f) => f.event.kind === kind)!;
      const suffix =
        kind === "started"
          ? first + ":start"
          : kind === "finished"
            ? "result-" + first
            : fact.sequence;
      assert.equal(fact.report_key, `claude:${fact.host_id}:${suffix}`);
    }
  },
);

for (const transient of [false, true]) {
  test(
    `permission registration ${transient ? "retries the original observation" : "denies the waiting callback on conflict"}`,
    { timeout: 6000 },
    async (t) => {
      const h = await setup(t, "interaction_requested", transient);
      await h.command({ method: "input", input_id: first, text: "task" });
      h.fixture.messages.push(init());
      h.fixture.messages.push(assistant());
      await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
      const response = h.fixture.options!.canUseTool!(
        "Write",
        {},
        {
          signal: new AbortController().signal,
          toolUseID: "tool",
          requestId: "native-request",
        },
      );
      if (transient) {
        await h.wait(
          () => h.facts.filter((f) => f.event.kind === "interaction_requested").length === 2,
        );
        await h.command({
          method: "interaction",
          interaction_key: "native-request",
          option: "allow",
        });
      }
      assert.equal((await response)?.behavior, transient ? "allow" : "deny");
      const registrations = h.facts.filter((f) => f.event.kind === "interaction_requested");
      assert.equal(registrations[0]!.event.interaction_key, "native-request");
      assert.equal(
        registrations[0]!.report_key,
        `claude:${registrations[0]!.host_id}:native-request`,
      );
      assert.deepEqual(registrations, Array(transient ? 2 : 1).fill(registrations[0]));
    },
  );
}

test(
  "settlement retries independently after a single delivered choice",
  { timeout: 6000 },
  async (t) => {
    const h = await setup(t, "interaction_settled", true);
    await h.command({ method: "input", input_id: first, text: "task" });
    h.fixture.messages.push(init());
    h.fixture.messages.push(assistant());
    await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
    const response = h.fixture.options!.canUseTool!(
      "Write",
      {},
      {
        signal: new AbortController().signal,
        toolUseID: "tool",
        requestId: "native-request",
      },
    );
    await h.wait(() => h.facts.some((f) => f.event.kind === "interaction_requested"));
    await h.command({ method: "interaction", interaction_key: "native-request", option: "allow" });
    assert.equal((await response)?.behavior, "allow");
    const settlements = () => h.facts.filter((f) => f.event.kind === "interaction_settled");
    await h.wait(() => settlements().length === 2);
    assert.deepEqual(settlements()[0], settlements()[1]);
    assert.equal(h.facts.filter((f) => f.event.kind === "interaction_requested").length, 1);
  },
);

test(
  "a terminal Session discards facts produced during the outage",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t);
    await h.command({ method: "input", input_id: first, text: "task" });
    h.fixture.messages.push(init());
    h.fixture.messages.push(assistant());
    await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
    await h.reconnect(async () => {
      h.endSession();
      h.fixture.messages.push(result());
    });
    assert.ok(!h.facts.some((f) => f.event.kind === "finished"));
  },
);

test(
  "pending and newly arrived offline permissions keep the original SDK callbacks",
  { timeout: 5000 },
  async (t) => {
    const h = await setup(t);
    await h.command({ method: "input", input_id: first, text: "task" });
    h.fixture.messages.push(init());
    h.fixture.messages.push(assistant());
    await h.wait(() => h.facts.some((f) => f.event.kind === "started"));
    const request = (id: string) =>
      h.fixture.options!.canUseTool!(
        "Write",
        { file_path: id },
        { signal: new AbortController().signal, toolUseID: id, requestId: id },
      );
    const firstAnswer = request("before");
    await h.wait(() => h.facts.some((f) => f.event.interaction_key === "before"));
    let secondAnswer: ReturnType<typeof request>;
    await h.reconnect(async () => {
      secondAnswer = request("during");
    });
    await h.wait(
      () => h.facts.filter((f) => f.event.kind === "interaction_requested").length === 3,
    );
    const before = h.facts.filter((f) => f.event.interaction_key === "before");
    assert.deepEqual(before[0], before[1]);
    for (const id of ["before", "during"]) {
      assert.equal(
        (await h.command({ method: "interaction", interaction_key: id, option: "allow" })).status,
        "accepted",
      );
    }
    for (const answer of [firstAnswer, secondAnswer!])
      assert.equal((await answer)?.behavior, "allow");
  },
);
