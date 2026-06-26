#!/usr/bin/env node
import { appendFile, mkdir, readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

const command = process.argv[2];
const env = process.env;

function stringValue(value) {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function pontiaHome() {
  return stringValue(env.PONTIA_HOME) ?? join(stringValue(env.HOME) ?? homedir(), ".pontia");
}

function logFile() {
  return join(pontiaHome(), "state", "claude-hook.log");
}

async function internalEventUrlFromPontiaHome() {
  let raw;
  try {
    raw = await readFile(join(pontiaHome(), "config.toml"), "utf8");
  } catch {
    return undefined;
  }
  const match = raw.match(/^\s*bind_addr\s*=\s*"([^"]*)"/m);
  const bindAddr = stringValue(match?.[1]);
  if (!bindAddr) return undefined;
  const parts = bindAddr.match(/^([^:]+):(\d+)$/);
  if (!parts) return undefined;
  const host = parts[1] === "0.0.0.0" || parts[1] === "::" ? "127.0.0.1" : parts[1];
  const port = parts[2];
  const baseUrl = port === "80" ? `http://${host}` : `http://${host}:${port}`;
  return `${baseUrl}/internal/v1/events`;
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

async function loadSessionContext() {
  const sessionId = stringValue(env.PONTIA_SESSION_ID);
  const runtimeInstanceId = stringValue(env.PONTIA_RUNTIME_INSTANCE_ID);
  const internalEventUrl = await internalEventUrlFromPontiaHome();
  const errors = [];
  if (!sessionId) errors.push("PONTIA_SESSION_ID is required");
  if (!runtimeInstanceId) errors.push("PONTIA_RUNTIME_INSTANCE_ID is required");
  if (!internalEventUrl) errors.push("pontia connection from PONTIA_HOME/config.toml is required");

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
