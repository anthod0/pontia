// Private IPC substitute. File gates control execution independently of control receipts.
import { registerHooks } from "node:module";
registerHooks({ resolve(specifier, context, nextResolve) {
  if (context.parentURL?.includes("/clients/shared/") && specifier.endsWith(".js")) specifier = specifier.slice(0, -3) + ".ts";
  return nextResolve(specifier, context);
}});
const { connectRuntime, CONTROL_VERSION } = await import("../../../../clients/shared/control-socket.ts");
import { randomUUID } from "node:crypto";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { execFileSync } from "node:child_process";

const lines = createInterface({ input: process.stdin });
const line = await new Promise((resolve) => lines.once("line", resolve));
lines.close();
const config = JSON.parse(line);
if (process.argv[2] === "history") {
  try {
    const messages = readFileSync(join(config.cwd, `history-${config.session_id}.jsonl`), "utf8")
      .trim()
      .split("\n")
      .map(JSON.parse);
    process.stdout.write(JSON.stringify(messages));
  } catch {
    process.exitCode = 1;
  }
} else {
  const host = randomUUID();
  const native = config.resume ?? randomUUID();
  const history = (message) =>
    appendFileSync(
      join(config.cwd, `history-${native}.jsonl`),
      JSON.stringify({ session_id: native, parent_tool_use_id: null, ...message }) + "\n",
    );
  let sequence = 0;
  let active;
  let lastInput;
  let closed = false;
  let compacting;
  let number = existsSync(join(config.cwd, "wire.jsonl"))
    ? readFileSync(join(config.cwd, "wire.jsonl"), "utf8")
        .trim()
        .split("\n")
        .map(JSON.parse)
        .filter((v) => v.session === config.session_id && v.method === "input").length
    : 0;
  let ready = false;
  let pendingReady;
  let connection;
  let connected = false;
  const reports = new Set();
  let respond;
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
  const send = (value) => respond({result: value});
  function hostObservation(host, sequence, event) {
    let suffix;
    if (event.kind === "started") suffix = event.input_id + ":start";
    else if (event.kind === "permission_requested") suffix = event.permission_id;
    else if (event.kind === "permission_settled") {
      event.authorization_digest = "a".repeat(64);
      return {host_id:host, sequence, event, registration_key:`claude:${host}:${event.permission_id}`,report_key:`claude:${host}:${event.permission_id}:settled`};
    }
    else if (event.kind === "finished") suffix = event.result_id;
    else if (["ready", "model", "context_usage", "compaction"].includes(event.kind)) suffix = sequence;
    return {host_id: host, sequence, report_key: suffix && `claude:${host}:${suffix}`, event};
  }
  const captured = [];
  let capture = false;
  const event = (value) => {
    sequence++;
    if (!connected) return;
    const params = hostObservation(host, sequence, value);
    const report = connection.request("host.event", params);
    if (capture && params.report_key) captured.push({ params, report });
    reports.add(report);
    void report.catch(() => {}).finally(() => reports.delete(report));
    if (value.kind === "ready") connection.registered({sessionId: config.session_id, runtimeId: config.runtime_id, clientSessionKey: native});
    return report;
  };
  const estimate = (used) => event({
    kind: "context_usage",
    context_usage: {
      used_tokens: used, max_tokens: 200000, remaining_tokens: 200000 - used,
      usage_ratio: used / 200000, confidence: "estimated",
    },
  });
  record({
    method: "spawn",
    host,
    system_prompt: config.system_prompt ?? null,
    execution_policy: config.execution_policy,
    resume: config.resume ?? null,
    native,
    pid: process.pid,
  });
  const startupError = join(config.cwd, "startup-error.json");
  if (existsSync(startupError)) {
    process.stdout.write(
      JSON.stringify({ listening: false, ...JSON.parse(readFileSync(startupError, "utf8")) }) +
        "\n",
    );
    process.exit(1);
  }
  const command = async (request, reply) => {
      respond = reply;
      record(request);
      if (request.method === "input") {
        number++;
        if (request.text === "unknown") process.exit(0);
        active = { id: request.input_id, text: request.text, input:request.input, number };
        lastInput = active.id;
        capture = active.text === "capture reports";
        history({ type: "user", uuid: active.id, message: { role: "user", content: active.text } });
        if (!ready) {
          if (["hold first ready","lose first ready receipt"].includes(active.text)) {
            pendingReady = hostObservation(host, ++sequence, {kind:"ready",native_session_id:native});
            if (active.text === "lose first ready receipt") await connection.request("host.event",pendingReady);
            connection.registered({sessionId:config.session_id,runtimeId:config.runtime_id,clientSessionKey:native});
          } else event({ kind: "ready", native_session_id: native });
          if (!pendingReady) event({ kind: "model", model: config.model ?? "sonnet" });
          if (config.resume && !pendingReady) estimate(664);
          ready = true;
        }
        // Receipt is deliberately delivered before execution begins.
        send({ type: "response", id: request.id, status: "accepted" });
        if (active.text === "replay reports") {
          for (const { params, report } of captured) {
            const original = await report;
            const replay = await connection.request("host.event", params);
            record({method: "replayed", params, original, replay});
          }
          const model = captured.find(({ params }) => params.event.kind === "model").params;
          try {
            await connection.request("host.event", {...model, event: {kind: "model", model: "changed"}});
          } catch (error) {
            record({method: "replay_conflict", code: error.code});
          }
          const finished = captured.find(({ params }) => params.event.kind === "finished").params;
          try {
            await connection.request("host.event", {...finished, event: {...finished.event, input_id: active.id}});
          } catch (error) {
            record({method: "result_conflict", code: error.code});
          }
          const changedOutcome = await connection.request("host.event", {...finished, event: {...finished.event, outcome: "failed"}});
          const lateParams = hostObservation(host, ++sequence, {
            ...finished.event, outcome: "failed", result_id: randomUUID(),
          });
          const late = await connection.request("host.event", lateParams);
          const repeated = await connection.request("host.event", lateParams);
          record({method: "late_result", late, repeated, changedOutcome});
        }
      } else if (request.method === "compact") {
        if (active || compacting || !ready) {
          send({ type: "response", id: request.id, status: "rejected" });
          return;
        }
        compacting = request.operation_id;
        event({ kind: "compaction", phase: "started", data: {operation_id: compacting, trigger: "manual"} });
        send({ type: "response", id: request.id, status: "accepted" });
      } else if (request.method === "steer") {
        if (!active?.started || request.target_input_id !== active.id) {
          send({ type: "response", id: request.id, status: "rejected" });
          return;
        }
        history({
          type: "user",
          uuid: request.input_id,
          message: { role: "user", content: request.text },
        });
        active.reply = randomUUID();
        history({
          type: "assistant",
          uuid: active.reply,
          message: { role: "assistant", content: [{ type: "text", text: request.text }] },
        });
        event({
          kind: "input",
          input_id: active.id,
          item_id: request.input_id,
          text: request.text,
        });
        event({
          kind: "text",
          input_id: active.id,
          item_id: request.input_id,
          delta: request.text,
        });
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
      } else if (request.method === "is_idle") {
        send({ type: "response", id: request.id, status: "accepted", idle: !active && !compacting });
      } else if (request.method === "close") {
        clearInterval(timer);
        expire();
        closed = true;
        event({ kind: "closed", history_anchor: active?.reply ?? active?.id });
        await Promise.allSettled(reports);
        reply({ result: {status: "accepted"} }, () => { void connection.close(); });
      }
  };
  const timer = setInterval(() => {
    if (!existsSync(config.cwd)) process.exit(0);
    if (compacting && existsSync(join(config.cwd, `compact-fail-${config.session_id}-1`))) {
      event({ kind: "compaction", phase: "failed", data: { operation_id: compacting, trigger: "manual", aborted: false } });
      compacting = undefined;
    }
    if (compacting && existsSync(join(config.cwd, `compact-${config.session_id}-1`))) {
      event({ kind: "compaction", phase: "completed", data: { operation_id: compacting, trigger: "manual", native_compaction_id: randomUUID(), tokens_before: 20000, tokens_after: 2000 } });
      estimate(622);
      compacting = undefined;
    }
    if (!active) return;
    const gate = (kind) =>
      existsSync(join(config.cwd, `${kind}-${config.session_id}-${active.number}`));
    if (!active.started && gate("start")) {
      active.started = true;
      if (active.text === "read-session-environment") {
        record({
          method: "session_environment",
          value: execFileSync("sh", ["-c", 'printf %s "$PONTIA_SESSION_ID"'], {
            encoding: "utf8",
          }),
        });
      }
      if (active.text === "read-workflow-environment") {
        record({
          method: "workflow_environment",
          value: execFileSync(
            "sh",
            ["-c", 'printf "%s\\n%s" "$PONTIA_WORKFLOW_ID" "$PONTIA_WORKFLOW_PATCH_ID"'],
            { encoding: "utf8" },
          ).split("\n"),
        });
      }
      active.reply = randomUUID();
      history({
        type: "assistant",
        uuid: active.reply,
        message: { role: "assistant", content: [{ type: "text", text: active.text }] },
      });
      event({
        kind: "context_usage",
        context_usage: {
          used_tokens: 14710,
          max_tokens: 200000,
          remaining_tokens: 185290,
          usage_ratio: 0.07355,
          input_tokens: 14000,
          cache_tokens: 710,
          output_tokens: 73,
          confidence: "exact",
        },
      });
      event({ kind: "started", input_id: active.id, input:active.input });
      event({ kind: "text", input_id: active.id, item_id: "reply", delta: active.text });
      if (active.text.startsWith("approval")) {
        for (let n = 0; n < (active.text === "approval-pair" ? 2 : 1); n++) {
          const id = `permission-${active.number}-${n}`;
          permissions.add(id);
          const request = {
            kind: "permission_requested",
            authorization_digest: "a".repeat(64),
            permission_id: id,
            tool: "Write",
            blocking: active.text !== "approval-background",
          };
          event(request);
          if (active.text === "approval-conflict") {
            event({ ...request, authorization_digest: "b".repeat(64) }).catch((error) => {
              record({ method: "permission_conflict", code: error.code });
              expire();
            });
          }
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
      if (active.text === "unsupported")
        event({ kind: "unavailable", reason: "unsupported_result" });
      else
        event({
          kind: "finished",
          input_id: active.id,
          result_id: randomUUID(),
          history_anchor: active.reply,
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
  connection = await connectRuntime(config.pontia_home, error => record({method:"attach_error",code:error.code}), (method, params, reply, error) => {
    if (method === "host.status") {
      reply({ result: { host_id: host, pending_ready: pendingReady && {report_key:pendingReady.report_key,native_session_id:native}, native_session_id: ready ? native : null, input_id: active?.id ?? lastInput ?? null, started: !!active?.started, finished: !active, closed, closing: closed, unavailable: null,
        launch: {cwd: config.cwd, config_dir: process.env.CLAUDE_CONFIG_DIR, model: config.model, resume: config.resume} } });
    } else if (method === "host.command") { connected = true; void command(params, reply).catch(() => error(-32603, "failed")); }
    else error(-32601, "unknown");
  }, async () => { connected = true; record({method:"attached"});
    if (pendingReady) {
      await connection.request("host.event", pendingReady);
      pendingReady = undefined;
      record({method:"ready_replayed"});
    }
  }, () => { connected = false; expire(); }, true);
  await connection.request("runtime.attach", {version: CONTROL_VERSION, session_id: config.session_id, runtime_id: config.runtime_id, client_session_key: undefined});
  connection.registered({sessionId: config.session_id, runtimeId: config.runtime_id, clientSessionKey: undefined});
  connected = true;
  record({method:"attached"});
  process.once("SIGTERM", () => { clearInterval(timer); void connection.close(); });
}
