import { describe, expect, test } from "vitest";
import {
  agentEndWasInterrupted,
  assistantDeltaFromEvent,
  assistantTextFromMessage,
  errorMessageFromAgentEnd,
  isTranscriptBoundaryMessageUpdate,
  lastAssistantTextFromMessages,
} from "../src/pi-message.js";

describe("pi message helpers", () => {
  test("extracts assistant text from string and text-part messages", () => {
    expect(assistantTextFromMessage({ role: "assistant", content: "hello" })).toBe("hello");
    expect(assistantTextFromMessage({
      role: "assistant",
      content: [
        { type: "text", text: "hello" },
        { type: "tool_use", name: "ignored" },
        { type: "text", text: " world" },
      ],
    })).toBe("hello world");
    expect(assistantTextFromMessage({ role: "user", content: "ignore" })).toBeUndefined();
  });

  test("extracts assistant deltas and recognizes transcript boundaries", () => {
    expect(assistantDeltaFromEvent({ assistantMessageEvent: { textDelta: "hi" } })).toBe("hi");
    expect(isTranscriptBoundaryMessageUpdate({ assistantMessageEvent: { type: "toolcall_start" } })).toBe(true);
    expect(isTranscriptBoundaryMessageUpdate({ assistantMessageEvent: { type: "text_delta" } })).toBe(false);
  });

  test("extracts final text and Pi agent end status", () => {
    expect(lastAssistantTextFromMessages([
      { role: "assistant", content: "first" },
      { role: "user", content: "ignore" },
      { role: "assistant", content: [{ type: "text", text: "last" }] },
    ])).toBe("last");
    expect(errorMessageFromAgentEnd({
      messages: [{ role: "assistant", stopReason: "error", errorMessage: "boom" }],
    })).toBe("boom");
    expect(errorMessageFromAgentEnd({
      messages: [{ role: "assistant", stopReason: "error" }],
    })).toBe("pi agent reported an error");
    expect(agentEndWasInterrupted({
      messages: [{ role: "assistant", stopReason: "aborted" }],
    })).toBe(true);
  });
});
