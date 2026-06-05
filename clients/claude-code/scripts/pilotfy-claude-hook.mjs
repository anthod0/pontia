#!/usr/bin/env node
import { appendFile, mkdir, readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const command = process.argv[2];
const env = process.env;

function runtimeDir() {
  return env.PILOTFY_RUNTIME_DIR ?? join(tmpdir(), "pilotfy", "claude-runtime-fallback");
}

function currentTurnFile() {
  return env.PILOTFY_CURRENT_TURN_FILE ?? join(runtimeDir(), "current-turn.json");
}

function logFile() {
  return env.PILOTFY_CLAUDE_HOOK_LOG ?? join(runtimeDir(), "claude-hook.log");
}

async function appendDiagnostic(entry) {
  const file = logFile();
  try {
    await mkdir(dirname(file), { recursive: true });
    await appendFile(file, `${JSON.stringify({ time: new Date().toISOString(), ...entry })}\n`, "utf8");
  } catch (error) {
    console.error("pilotfy Claude Code plugin diagnostic write failed", error);
  }
}

async function stdinJson() {
  let raw = "";
  for await (const chunk of process.stdin) raw += chunk;
  if (!raw.trim()) return {};
  try {
    return JSON.parse(raw);
  } catch (error) {
    await appendDiagnostic({ level: "error", code: "invalid_hook_payload", message: "hook payload is not valid JSON", details: error instanceof Error ? error.message : String(error) });
    return {};
  }
}

function stringValue(value) {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function hasPilotfyRuntimeIntent() {
  return Boolean(
    stringValue(env.PILOTFY_RUNTIME_DIR) ||
      stringValue(env.PILOTFY_CURRENT_TURN_FILE) ||
      stringValue(env.PILOTFY_SESSION_ID) ||
      stringValue(env.PILOTFY_RUNTIME_INSTANCE_ID) ||
      stringValue(env.PILOTFY_INTERNAL_EVENT_URL),
  );
}

async function loadSessionContext() {
  const sessionId = stringValue(env.PILOTFY_SESSION_ID);
  const runtimeInstanceId = stringValue(env.PILOTFY_RUNTIME_INSTANCE_ID);
  const internalEventUrl = stringValue(env.PILOTFY_INTERNAL_EVENT_URL);
  const errors = [];
  if (!sessionId) errors.push("PILOTFY_SESSION_ID is required");
  if (!runtimeInstanceId) errors.push("PILOTFY_RUNTIME_INSTANCE_ID is required");
  if (!internalEventUrl) errors.push("PILOTFY_INTERNAL_EVENT_URL is required");
  if (errors.length) {
    if (!hasPilotfyRuntimeIntent()) return undefined;
    await appendDiagnostic({ level: "error", code: "invalid_session_context", message: errors.join("; ") });
    return undefined;
  }
  return { sessionId, runtimeInstanceId, internalEventUrl };
}

async function loadContext() {
  const file = currentTurnFile();
  let parsed;
  try {
    parsed = JSON.parse(await readFile(file, "utf8"));
  } catch (error) {
    if (!hasPilotfyRuntimeIntent()) return undefined;
    await appendDiagnostic({ level: "warn", code: "missing_current_turn_file", message: `current-turn file is missing, unreadable, or invalid: ${file}`, details: error instanceof Error ? error.message : String(error) });
    return undefined;
  }
  const sessionId = stringValue(parsed?.session_id);
  const turnId = stringValue(parsed?.turn_id);
  const clientType = stringValue(parsed?.client_type);
  const internalEventUrl = stringValue(env.PILOTFY_INTERNAL_EVENT_URL) ?? stringValue(parsed?.internal_event_url);
  const errors = [];
  if (!sessionId) errors.push("session_id is required");
  if (!turnId) errors.push("turn_id is required");
  if (clientType !== "claude_code") errors.push("client_type must be claude_code");
  if (!internalEventUrl) errors.push("internal_event_url or PILOTFY_INTERNAL_EVENT_URL is required");
  if (errors.length) {
    await appendDiagnostic({ level: "error", code: "invalid_current_turn_context", message: errors.join("; "), details: { contextFile: file } });
    return undefined;
  }
  return { sessionId, turnId, internalEventUrl };
}

function event(context, type, payload) {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: type === "session.ready" ? null : context.turnId,
    source: type === "session.ready" ? "agent_client" : "agent_adapter",
    client_type: "claude_code",
    type,
    time: new Date().toISOString(),
    seq: null,
    payload,
  };
}

async function postEvent(context, evt) {
  try {
    const response = await fetch(context.internalEventUrl, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(evt) });
    if (!response.ok) {
      const body = await response.text().catch(() => "");
      await appendDiagnostic({ level: "error", code: "internal_event_post_failed", message: `Internal Event API rejected ${evt.type}: ${response.status} ${response.statusText}`, details: { event_id: evt.event_id, status: response.status, body } });
      return false;
    }
    return true;
  } catch (error) {
    await appendDiagnostic({ level: "error", code: "internal_event_post_exception", message: `Internal Event API POST failed for ${evt.type}`, details: { event_id: evt.event_id, error: error instanceof Error ? error.message : String(error) } });
    return false;
  }
}

function failureMessage(payload) {
  const error = typeof payload?.error === "string" && payload.error.trim() ? payload.error.trim() : "unknown_error";
  const details = typeof payload?.error_details === "string" && payload.error_details.trim()
    ? payload.error_details.trim()
    : typeof payload?.last_assistant_message === "string" && payload.last_assistant_message.trim()
      ? payload.last_assistant_message.trim()
      : undefined;
  return details ? `Claude Code failed: ${error} - ${details}` : `Claude Code failed: ${error}`;
}

try {
  const payload = await stdinJson();
  if (command === "session-start") {
    const context = await loadSessionContext();
    if (context) await postEvent(context, event(context, "session.ready", { runtime_instance_id: context.runtimeInstanceId }));
    process.exit(0);
  }

  const context = await loadContext();
  if (context && command === "stop") {
    const output = typeof payload?.last_assistant_message === "string" ? payload.last_assistant_message.trim() : "";
    if (output && !(await postEvent(context, event(context, "turn.output", { output: { summary: output } })))) process.exit(0);
    await postEvent(context, event(context, "turn.completed", {}));
  } else if (context && command === "stop-failure") {
    await postEvent(context, event(context, "turn.failed", { failure: { message: failureMessage(payload) } }));
  } else if (context && command !== "prompt-submit") {
    await appendDiagnostic({ level: "warn", code: "unknown_hook_command", message: `unknown Claude Code hook command: ${command}` });
  }
} catch (error) {
  await appendDiagnostic({ level: "error", code: "unexpected_hook_exception", message: "Claude Code hook handling failed", details: error instanceof Error ? error.message : String(error) });
}
process.exit(0);
