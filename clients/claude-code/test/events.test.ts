import { describe, expect, test, vi } from "vitest";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach } from "vitest";
import type { TurnContext } from "../src/context.js";
import { buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnOutputEvent } from "../src/events.js";
import { EventReporter } from "../src/reporter.js";

const tmpDirs: string[] = [];

afterEach(async () => {
  await Promise.all(tmpDirs.map((dir) => rm(dir, { recursive: true, force: true })));
  tmpDirs.length = 0;
});

async function tempLogFile() {
  const dir = await mkdtemp(join(tmpdir(), "pilotfy-claude-reporter-"));
  tmpDirs.push(dir);
  return join(dir, "claude-hook.log");
}

const context: TurnContext = {
  sessionId: "sess_1",
  turnId: "turn_1",
  clientType: "claude_code",
  internalEventUrl: "http://127.0.0.1:8080/internal/v1/events",
};

describe("event builders", () => {
  test("builds claude_code turn.output payload shape", () => {
    const event = buildTurnOutputEvent(context, "hello");

    expect(event.event_id).toMatch(/^evt_/);
    expect(event).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      source: "agent_adapter",
      client_type: "claude_code",
      type: "turn.output",
      seq: null,
      payload: { output: { summary: "hello" } },
    });
    expect(new Date(event.time).toISOString()).toBe(event.time);
  });

  test("builds turn.completed and turn.failed payload shapes", () => {
    expect(buildTurnCompletedEvent(context)).toMatchObject({ type: "turn.completed", payload: {} });
    expect(buildTurnFailedEvent(context, "Claude Code failed: rate_limit - 429")).toMatchObject({
      type: "turn.failed",
      payload: { failure: { message: "Claude Code failed: rate_limit - 429" } },
    });
  });
});

describe("EventReporter", () => {
  test("posts event JSON to the Internal Event API", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ accepted: true }), { status: 202 }));
    const reporter = new EventReporter({ fetch, logFile: await tempLogFile() });
    const event = buildTurnCompletedEvent(context);

    const ok = await reporter.report(context, event);

    expect(ok).toBe(true);
    expect(fetch).toHaveBeenCalledWith(context.internalEventUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(event),
    });
  });

  test("logs POST failures and returns false", async () => {
    const logFile = await tempLogFile();
    const fetch = vi.fn(async () => new Response("nope", { status: 500, statusText: "Server Error" }));
    const reporter = new EventReporter({ fetch, logFile });

    const ok = await reporter.report(context, buildTurnCompletedEvent(context));

    expect(ok).toBe(false);
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("internal_event_post_failed");
    expect(log).toContain("500");
  });
});
