import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { once } from "node:events";
import { chmod, mkdtempDisposable, stat } from "node:fs/promises";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test, type TestContext } from "node:test";
import { serve, type Bootstrap } from "../src/server.js";
import { assistant, first, init, result, sdk, second } from "./fixture.js";

async function setup(t: TestContext) {
  const sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-host-test-"));
  const testRoot = sandbox.path;
  const fixture = sdk();
  const config: Bootstrap = {
    session_id: "session",
    runtime_id: "runtime",
    socket_path: join(testRoot, "host.sock"),
    cwd: testRoot,
    token: randomBytes(32).toString("hex"),
  };
  const server = await serve(config, fixture.factory);
  t.after(async () => {
    try {
      fixture.messages.end();
      await server.close();
    } finally {
      await sandbox.remove();
    }
  });
  return { config, fixture };
}

type Frame = Record<string, any>;
async function peer(config: Bootstrap) {
  const socket = connect(config.socket_path);
  const frames: Frame[] = [];
  const waits: Array<{ match: (frame: Frame) => boolean; resolve: (frame: Frame) => void }> = [];
  let buffer = "";
  socket.on("error", () => {});
  socket.on("data", (chunk) => {
    buffer += chunk.toString();
    while (buffer.includes("\n")) {
      const newline = buffer.indexOf("\n");
      const frame: Frame = JSON.parse(buffer.slice(0, newline));
      buffer = buffer.slice(newline + 1);
      const index = waits.findIndex((wait) => wait.match(frame));
      if (index >= 0) waits.splice(index, 1)[0]!.resolve(frame);
      else frames.push(frame);
    }
  });
  await once(socket, "connect");
  return {
    socket,
    frames,
    send: (frame: unknown) => socket.write(JSON.stringify(frame) + "\n"),
    next(match: (frame: Frame) => boolean): Promise<Frame> {
      const index = frames.findIndex(match);
      if (index >= 0) return Promise.resolve(frames.splice(index, 1)[0]!);
      return new Promise((resolve) => waits.push({ match, resolve }));
    },
  };
}
async function attach(
  config: Bootstrap,
  checkpoint: { last_host_id?: string; last_sequence?: number } = {},
) {
  const connection = await peer(config);
  connection.send({
    method: "attach",
    token: config.token,
    session_id: config.session_id,
    runtime_id: config.runtime_id,
    ...checkpoint,
  });
  const attached = await connection.next((frame) => frame.type === "attached");
  return { ...connection, host: attached.host_id as string, attached };
}

test(
  "private socket rejects incorrect capability or runtime before accepting any input",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    assert.equal((await stat(config.socket_path)).mode & 0o777, 0o600);
    for (const change of [{ token: "wrong" }, { runtime_id: "other-runtime" }]) {
      const connection = await peer(config);
      const closed = once(connection.socket, "close");
      connection.send({
        method: "attach",
        token: config.token,
        session_id: config.session_id,
        runtime_id: config.runtime_id,
        ...change,
      });
      await closed;
    }
    assert.equal(fixture.calls, 0);
    const connection = await attach(config);
    assert.equal(connection.attached.native_session_id, null);
  },
);

test(
  "duplicate requests execute only once and reused IDs with changed content are rejected",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    const connection = await attach(config);
    const request = {
      host_id: connection.host,
      id: 1,
      method: "input",
      input_id: first,
      text: "actual input",
    };
    connection.send(request);
    connection.send(request);
    assert.equal(
      (await connection.next((f) => f.type === "response" && f.id === 1)).status,
      "accepted",
    );
    assert.equal(
      (await connection.next((f) => f.type === "response" && f.id === 1)).status,
      "accepted",
    );
    assert.equal((await fixture.next()).value.uuid, first);
    assert.equal(fixture.calls, 1);
    connection.send({ ...request, text: "different input" });
    assert.equal((await connection.next((f) => f.type === "error")).reason, "request_id_reused");
    fixture.messages.push(init());
    fixture.messages.push(assistant());
    fixture.messages.push(result());
    const finished = await connection.next((f) => f.event?.kind === "finished");
    assert.equal(finished.event.input_id, first);
    connection.send({
      host_id: connection.host,
      id: 2,
      method: "input",
      input_id: second,
      text: "next task",
    });
    assert.equal(
      (await connection.next((f) => f.type === "response" && f.id === 2)).status,
      "accepted",
    );
    assert.equal((await fixture.next()).value.uuid, second);
  },
);

test(
  "a control replacement fences old replies and does not replay input or infer task completion",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    const old = await attach(config);
    old.send({ host_id: old.host, id: 1, method: "input", input_id: first, text: "task" });
    await old.next((f) => f.type === "response");
    await fixture.next();
    fixture.messages.push(init());
    fixture.messages.push(assistant());
    await old.next((f) => f.event?.kind === "started");
    const closed = once(old.socket, "close");
    const current = await attach(config);
    await closed;
    assert.equal(current.host, old.host);
    assert.equal(current.attached.input_id, first);
    assert.equal(current.attached.finished, false);
    assert.equal(current.attached.unavailable, "control_connection_lost");
    current.send({
      host_id: current.host,
      id: 2,
      method: "input",
      input_id: second,
      text: "not replayed",
    });
    assert.equal((await current.next((f) => f.type === "response")).status, "rejected");
    current.send({ host_id: current.host, id: 3, method: "interrupt", input_id: first });
    assert.equal((await current.next((f) => f.type === "response")).status, "accepted");
    assert.equal(fixture.interrupts, 1);
    assert.equal(fixture.calls, 1);
  },
);

test(
  "replacing a connection before first input leaves input available",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    const old = await attach(config);
    const closed = once(old.socket, "close");
    const current = await attach(config);
    await closed;
    assert.equal(current.attached.unavailable, null);
    current.send({
      host_id: current.host,
      id: 1,
      method: "input",
      input_id: first,
      text: "first task",
    });
    assert.equal((await current.next((f) => f.type === "response")).status, "accepted");
    assert.equal((await fixture.next()).value.uuid, first);
  },
);

test(
  "a controller caught up with all host facts can reconnect and submit the next task",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    const old = await attach(config);
    old.send({ host_id: old.host, id: 1, method: "input", input_id: first, text: "task" });
    await old.next((f) => f.type === "response");
    await fixture.next();
    fixture.messages.push(init());
    fixture.messages.push(assistant());
    fixture.messages.push(result());
    const finished = await old.next((f) => f.event?.kind === "finished");
    const current = await attach(config, {
      last_host_id: old.host,
      last_sequence: finished.sequence,
    });
    assert.equal(current.attached.unavailable, null);
    current.send({ host_id: current.host, id: 2, method: "input", input_id: second, text: "next" });
    assert.equal((await current.next((f) => f.type === "response")).status, "accepted");
    assert.equal((await fixture.next()).value.uuid, second);
    assert.equal(fixture.calls, 1);
  },
);

test(
  "a stale fact cursor cannot resume input after a missed result",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    const old = await attach(config);
    old.send({ host_id: old.host, id: 1, method: "input", input_id: first, text: "task" });
    await old.next((f) => f.type === "response");
    fixture.messages.push(init());
    fixture.messages.push(assistant());
    const started = await old.next((f) => f.event?.kind === "started");
    fixture.messages.push(result());
    await old.next((f) => f.event?.kind === "finished");
    const current = await attach(config, {
      last_host_id: old.host,
      last_sequence: started.sequence,
    });
    assert.equal(current.attached.unavailable, "control_connection_lost");
    current.send({ host_id: current.host, id: 2, method: "input", input_id: second, text: "next" });
    assert.equal((await current.next((f) => f.type === "response")).status, "rejected");
  },
);

test(
  "close shuts down the host even when the requesting connection was replaced",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    const old = await attach(config);
    old.send({ host_id: old.host, id: 1, method: "input", input_id: first, text: "task" });
    await old.next((f) => f.type === "response");
    let beginClose!: () => void;
    const closing = new Promise<void>((resolve) => {
      beginClose = resolve;
    });
    fixture.onClose(beginClose);
    old.send({ host_id: old.host, id: 2, method: "close" });
    await closing;
    const current = await attach(config);
    const closed = once(current.socket, "close");
    fixture.messages.end();
    await current.next((f) => f.event?.kind === "closed");
    await closed;
    await assert.rejects(peer(config));
  },
);

test(
  "interrupt handling does not block an independent close request",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    const connection = await attach(config);
    connection.send({
      host_id: connection.host,
      id: 1,
      method: "input",
      input_id: first,
      text: "task",
    });
    await connection.next((f) => f.type === "response");
    fixture.messages.push(init());
    fixture.messages.push(assistant());
    await connection.next((f) => f.event?.kind === "started");
    let release!: () => void;
    fixture.onInterrupt(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    connection.send({ host_id: connection.host, id: 2, method: "interrupt", input_id: first });
    connection.send({ host_id: connection.host, id: 3, method: "close" });
    const response = await connection.next((f) => f.type === "response" && f.id === 3);
    assert.equal(response.status, "accepted");
    assert.equal(connection.frames.filter((f) => f.event?.kind === "finished").length, 0);
    release();
  },
);

test(
  "an expired duplicate is rejected rather than executing again after receipt eviction",
  { timeout: 5000 },
  async (t) => {
    const { config, fixture } = await setup(t);
    const connection = await attach(config);
    const request = {
      host_id: connection.host,
      id: 1,
      method: "input",
      input_id: first,
      text: "task",
    };
    connection.send(request);
    await connection.next((f) => f.type === "response");
    for (let id = 2; id < 132; id += 1) {
      connection.send({ host_id: connection.host, id, method: "unknown" });
      await connection.next((f) => f.type === "response" && f.id === id);
    }
    connection.send(request);
    assert.equal((await connection.next((f) => f.type === "error")).reason, "request_expired");
    assert.equal(fixture.calls, 1);
  },
);

test(
  "the ordinary Node host boots over stdin and exits after an authenticated close",
  { timeout: 5000 },
  async () => {
    await using sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-host-cli-"));
    const config: Bootstrap = {
      session_id: "session",
      runtime_id: "runtime",
      socket_path: join(sandbox.path, "host.sock"),
      cwd: sandbox.path,
      token: randomBytes(32).toString("hex"),
    };
    const child = spawn(
      process.execPath,
      [fileURLToPath(new URL("../src/main.js", import.meta.url))],
      {
        cwd: sandbox.path,
        stdio: ["pipe", "pipe", "pipe"],
      },
    );
    const exit = once(child, "exit");
    try {
      const listening = once(child.stdout, "data");
      child.stdin.end(JSON.stringify(config) + "\n");
      await listening;
      const connection = await attach(config);
      connection.send({ host_id: connection.host, id: 1, method: "close" });
      assert.equal((await connection.next((f) => f.type === "response")).status, "accepted");
      assert.deepEqual(await exit, [0, null]);
    } finally {
      // Only our ordinary fixture host; it never received an SDK input.
      if (child.exitCode === null && child.signalCode === null) {
        child.kill();
        await exit;
      }
    }
  },
);

test("a non-private socket directory is refused without changing its permissions", async () => {
  await using sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-host-permissions-"));
  const testRoot = sandbox.path;
  await chmod(testRoot, 0o755);
  await assert.rejects(
    serve(
      {
        session_id: "s",
        runtime_id: "r",
        socket_path: join(testRoot, "host.sock"),
        cwd: testRoot,
        token: randomBytes(32).toString("hex"),
      },
      sdk().factory,
    ),
  );
  assert.equal((await stat(testRoot)).mode & 0o777, 0o755);
});
