import assert from "node:assert/strict";
import test from "node:test";
import type { SessionMessage } from "@anthropic-ai/claude-agent-sdk";
import { forkHistory } from "../src/fork.js";

function message(session: string, uuid: string, type: "user" | "assistant"): SessionMessage {
  return {
    type,
    uuid,
    session_id: session,
    message: {},
    parent_tool_use_id: null,
    parent_agent_id: null,
  };
}

test("forks immediately before the edited message and maps copied history IDs", async () => {
  const source = [
    message("source", "user-1", "user"),
    message("source", "assistant-1", "assistant"),
    message("source", "user-2", "user"),
    message("source", "assistant-2", "assistant"),
  ];
  const copied = [
    message("child", "child-user-1", "user"),
    message("child", "child-assistant-1", "assistant"),
  ];
  const calls: unknown[] = [];
  const result = await forkHistory("source", "user-2", ["user-1", "assistant-1"], "/repo", {
    messages: async (session, options) => {
      calls.push(["messages", session, options]);
      return session === "source" ? source : copied;
    },
    fork: async (session, options) => {
      calls.push(["fork", session, options]);
      return { sessionId: "child" };
    },
    uuid: () => "unused",
  });

  assert.deepEqual(result, {
    session_id: "child",
    resume: true,
    message_ids: { "user-1": "child-user-1", "assistant-1": "child-assistant-1" },
  });
  assert.deepEqual(calls, [
    ["messages", "source", { dir: "/repo" }],
    ["fork", "source", { dir: "/repo", upToMessageId: "assistant-1" }],
    ["messages", "child", { dir: "/repo" }],
  ]);
});

test("creates an empty native identity when editing the first message", async () => {
  let forked = false;
  const result = await forkHistory("source", "user-1", [], "/repo", {
    messages: async () => [message("source", "user-1", "user")],
    fork: async () => {
      forked = true;
      return { sessionId: "unexpected" };
    },
    uuid: () => "empty-child",
  });

  assert.deepEqual(result, { session_id: "empty-child", resume: false, message_ids: {} });
  assert.equal(forked, false);
});
