#!/usr/bin/env node
import { appendFile, mkdir } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { dirname } from "node:path";

const command = process.argv[2];
const env = process.env;

function logFile() {
  return typeof env.PONTIA_CLAUDE_HOOK_LOG === "string" && env.PONTIA_CLAUDE_HOOK_LOG.trim()
    ? env.PONTIA_CLAUDE_HOOK_LOG.trim()
    : undefined;
}

async function appendDiagnostic(entry) {
  const file = logFile();
  if (!file) return;
  try {
    await mkdir(dirname(file), { recursive: true });
    await appendFile(file, `${JSON.stringify({ time: new Date().toISOString(), ...entry })}\n`, "utf8");
  } catch (error) {
    console.error("pontia Claude Code plugin diagnostic write failed", error);
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
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

async function loadSessionContext() {
  const sessionId = stringValue(env.PONTIA_SESSION_ID);
  const runtimeInstanceId = stringValue(env.PONTIA_RUNTIME_INSTANCE_ID);
  const internalEventUrl = stringValue(env.PONTIA_INTERNAL_EVENT_URL);
  const errors = [];
  if (!sessionId) errors.push("PONTIA_SESSION_ID is required");
  if (!runtimeInstanceId) errors.push("PONTIA_RUNTIME_INSTANCE_ID is required");
  if (!internalEventUrl) errors.push("PONTIA_INTERNAL_EVENT_URL is required");
  if (!logFile()) errors.push("PONTIA_CLAUDE_HOOK_LOG is required");
  if (errors.length) {
    await appendDiagnostic({ level: "error", code: "invalid_session_context", message: errors.join("; ") });
    return undefined;
  }
  return { sessionId, runtimeInstanceId, internalEventUrl };
}

function event(context, type, payload) {
  return {
    event_id: `evt_${randomUUID()}`,
    session_id: context.sessionId,
    turn_id: null,
    source: "agent_client",
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

try {
  await stdinJson();
  if (command === "session-start") {
    const context = await loadSessionContext();
    if (context) await postEvent(context, event(context, "session.ready", { runtime_instance_id: context.runtimeInstanceId }));
  }
} catch (error) {
  await appendDiagnostic({ level: "error", code: "unexpected_hook_exception", message: "Claude Code hook handling failed", details: error instanceof Error ? error.message : String(error) });
}
process.exit(0);
