import { describe, expect, test } from "vitest";
import { buildTurnOutputEvent } from "../src/events.js";

const context = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "claude",
  internalEventUrl: "http://127.0.0.1:8080/internal/v1/events",
} as const;

describe("event builders", () => {
  test("truncates turn.output summaries to 200 Unicode characters", () => {
    const event = buildTurnOutputEvent(context, "界".repeat(201));

    expect(event.payload).toEqual({ output: { summary: "界".repeat(200) } });
  });
});
