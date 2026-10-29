// Private IPC substitute. File gates control execution independently of control receipts.
import { createServer } from "node:net";
import { randomUUID } from "node:crypto";
import { appendFileSync, existsSync, chmodSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";

const lines = createInterface({ input: process.stdin });
const [line] = await Promise.all([new Promise((resolve) => lines.once("line", resolve))]);
lines.close();
const config = JSON.parse(line);
const host = randomUUID();
const native = randomUUID();
let sequence = 0;
let active;
let number = 0;
let connected;
const permissions = new Set();
const expire = () => {
  for (const id of permissions)
    event({ kind: "permission_settled", permission_id: id, outcome: "expired" });
  permissions.clear();
};
const record = (value) =>
  appendFileSync(
    join(config.cwd, "wire.jsonl"),
    JSON.stringify({ session: config.session_id, ...value }) + "\n",
  );
const send = (value) => connected?.write(JSON.stringify({ host_id: host, ...value }) + "\n");
const event = (value) => send({ type: "event", sequence: ++sequence, event: value });
const server = createServer((socket) => {
  connected = socket;
  const reader = createInterface({ input: socket });
  reader.on("line", (line) => {
    const request = JSON.parse(line);
    if (request.method === "attach") {
      if (
        request.token !== config.token ||
        request.session_id !== config.session_id ||
        request.runtime_id !== config.runtime_id
      )
        return socket.destroy();
      send({ type: "attached", sequence: 0, high_water: 0, unavailable: null });
      return;
    }
    record(request);
    if (request.method === "input") {
      number++;
      if (request.text === "unknown") process.exit(0);
      active = { id: request.input_id, text: request.text, number };
      if (number === 1) event({ kind: "ready", native_session_id: native });
      // Receipt is deliberately delivered before execution begins.
      send({ type: "response", id: request.id, status: "accepted" });
    } else if (request.method === "permission") {
      if (active?.text === "approval-lost") process.exit(0);
      if (
        !permissions.delete(request.permission_id) ||
        !["allow", "deny"].includes(request.option)
      ) {
        send({ type: "response", id: request.id, status: "rejected" });
        return;
      }
      event({
        kind: "permission_settled",
        permission_id: request.permission_id,
        outcome: "resolved",
      });
      send({ type: "response", id: request.id, status: "accepted" });
    } else if (request.method === "interrupt") {
      active.interrupt = true;
      expire();
      send({ type: "response", id: request.id, status: "accepted" });
    } else if (request.method === "close") {
      clearInterval(timer);
      expire();
      event({ kind: "closed" });
      send({ type: "response", id: request.id, status: "accepted" });
      socket.end();
      server.close();
    }
  });
});
const timer = setInterval(() => {
  if (!active) return;
  const gate = (kind) =>
    existsSync(join(config.cwd, `${kind}-${config.session_id}-${active.number}`));
  if (!active.started && gate("start")) {
    active.started = true;
    event({ kind: "started", input_id: active.id });
    event({ kind: "text", input_id: active.id, item_id: "reply", delta: active.text });
    if (active.text.startsWith("approval")) {
      for (let n = 0; n < (active.text === "approval-pair" ? 2 : 1); n++) {
        const id = `permission-${active.number}-${n}`;
        permissions.add(id);
        event({
          kind: "permission_requested",
          permission_id: id,
          tool: "Write",
          blocking: active.text !== "approval-background",
        });
      }
    }
    if (active.text === "tool") {
      event({
        kind: "tool",
        input_id: active.id,
        call_id: "tool-call",
        name: "Read",
        arguments: { file_path: "test.txt" },
      });
      event({
        kind: "tool_result",
        input_id: active.id,
        call_id: "tool-call",
        failed: true,
        content: "denied",
      });
    }
  }
  if (gate("cancel")) expire();
  if (gate("lost")) process.exit(0);
  if (active.started && gate("finish")) {
    if (active.text === "unsupported") event({ kind: "unavailable", reason: "unsupported_result" });
    else
      event({
        kind: "finished",
        input_id: active.id,
        result_id: randomUUID(),
        outcome:
          active.text === "fail"
            ? "failed"
            : active.interrupt && active.text !== "completion-wins"
              ? "interrupted"
              : "completed",
      });
    active = undefined;
  }
}, 5);
server.listen(config.socket_path, () => {
  chmodSync(config.socket_path, 0o600);
  process.stdout.write('{"listening":true}\n');
});
