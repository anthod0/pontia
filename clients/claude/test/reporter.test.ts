import { describe, expect, test, vi } from "vitest";
import { EventReporter } from "../src/reporter.js";

describe("EventReporter", () => {
  test("sends a reported fact and returns pontia's canonical turn_id", async () => {
    const fetchImpl = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      expect(JSON.parse(String(init?.body))).toEqual({
        session_id: "sess_1",
        type: "turn.started",
        data: {
          runtime_instance_id: "rtinst_1",
          input: { summary: "build it" },
        },
      });
      return new Response(JSON.stringify({ accepted: true, turn_id: "turn_server" }), { status: 200 });
    });
    const reporter = new EventReporter({ fetch: fetchImpl as typeof fetch, logFile: "/tmp/unused-claude-hook.log" });

    const result = await reporter.report(
      { internalEventUrl: "http://localhost/internal/v1/events" },
      {
        session_id: "sess_1",
        type: "turn.started",
        data: {
          runtime_instance_id: "rtinst_1",
          input: { summary: "build it" },
        },
      },
    );

    expect(result).toEqual({ accepted: true, turnId: "turn_server" });
  });
});
