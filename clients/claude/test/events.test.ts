import { describe, expect, test } from "vitest";
import { buildTurnOutputEvent, buildTurnStartedEvent } from "../src/events.js";

const context = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "claude",
  internalEventUrl: "http://127.0.0.1:8080/internal/v1/events",
} as const;

describe("event builders", () => {
  test("omits client turn identity from turn.started facts", () => {
    const event = buildTurnStartedEvent(context);

    expect(event).toEqual({
      session_id: "sess_1",
      type: "turn.started",
      data: {
        runtime_instance_id: "rtinst_1",
        input: {},
      },
    });
  });

  test("truncates turn.output summaries to 200 Unicode characters", () => {
    const event = buildTurnOutputEvent(context, "界".repeat(201));

    expect(event.data).toEqual({ output: { summary: "界".repeat(200) } });
  });
});
