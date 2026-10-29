import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdir, mkdtempDisposable, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { Host, type QueryFactory } from "../src/host.js";
import type { HostEvent } from "../src/facts.js";

// Opt-in runs the real SDK against a loopback-only model substitute, never a live model.
test(
  "real SDK: first input, sequential task, close and same-ID resume through the host",
  {
    skip: process.env.PONTIA_CLAUDE_SDK_TEST !== "1",
    timeout: 60000,
  },
  async () => {
    await using sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-host-sdk-"));
    const testRoot = sandbox.path;
    const cwd = join(testRoot, "workspace");
    const config = join(testRoot, "config");
    const temporary = join(testRoot, "tmp");
    await Promise.all([cwd, config, temporary].map((path) => mkdir(path)));
    await writeFile(join(cwd, "input.txt"), "isolated tool result marker\n");
    const requests: Array<{ messages: unknown }> = [];
    const server = createServer(async (request, response) => {
      try {
        const chunks: Buffer[] = [];
        for await (const chunk of request) chunks.push(Buffer.from(chunk));
        const body = JSON.parse(Buffer.concat(chunks).toString() || "{}");
        if (request.url?.includes("count_tokens")) {
          response.setHeader("content-type", "application/json");
          response.end(JSON.stringify({ input_tokens: 12 }));
          return;
        }
        if (!request.url?.startsWith("/v1/messages")) {
          response.writeHead(404).end();
          return;
        }
        requests.push({ messages: body.messages });
        const message = {
          id: `message_${requests.length}`,
          type: "message",
          role: "assistant",
          model: body.model,
          content:
            requests.length === 1
              ? [
                  { type: "text", text: "reading" },
                  {
                    type: "tool_use",
                    id: "read-fixture",
                    name: "Read",
                    input: { file_path: join(cwd, "input.txt") },
                  },
                ]
              : [{ type: "text", text: "fixture answer" }],
          stop_reason: requests.length === 1 ? "tool_use" : "end_turn",
          stop_sequence: null,
          usage: { input_tokens: 12, output_tokens: 3 },
        };
        if (!body.stream) {
          response.setHeader("content-type", "application/json");
          response.end(JSON.stringify(message));
          return;
        }
        response.writeHead(200, { "content-type": "text/event-stream" });
        const send = (frame: { type: string; [key: string]: unknown }) =>
          response.write(`event: ${frame.type}\ndata: ${JSON.stringify(frame)}\n\n`);
        send({ type: "message_start", message: { ...message, content: [], stop_reason: null } });
        for (const [index, block] of message.content.entries()) {
          send({
            type: "content_block_start",
            index,
            content_block:
              block.type === "text" ? { type: "text", text: "" } : { ...block, input: {} },
          });
          send({
            type: "content_block_delta",
            index,
            delta:
              block.type === "text"
                ? { type: "text_delta", text: block.text }
                : { type: "input_json_delta", partial_json: JSON.stringify(block.input) },
          });
          send({ type: "content_block_stop", index });
        }
        send({
          type: "message_delta",
          delta: { stop_reason: message.stop_reason, stop_sequence: null },
          usage: { output_tokens: 3 },
        });
        send({ type: "message_stop" });
        response.end();
      } catch {
        response.writeHead(500).end();
      }
    });
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    assert.ok(address && typeof address !== "string");
    const env: Record<string, string | undefined> = Object.fromEntries(
      ["HOME", "USERPROFILE", "PATH", "LANG", "LC_ALL", "SystemRoot"].map((key) => [
        key,
        process.env[key],
      ]),
    );
    Object.assign(env, {
      CLAUDE_CONFIG_DIR: config,
      TMPDIR: temporary,
      ANTHROPIC_BASE_URL: `http://127.0.0.1:${address.port}`,
      ANTHROPIC_API_KEY: "fixture-not-a-secret",
      CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
      DISABLE_AUTOUPDATER: "1",
    });
    const factory: QueryFactory = (args) =>
      query({
        ...args,
        options: {
          ...args.options,
          env,
          settingSources: [],
          strictMcpConfig: true,
          mcpServers: {},
          plugins: [],
          model: "claude-sonnet-4-6",
          maxTurns: 4,
        },
      });
    const hosts: Host[] = [];
    const events: HostEvent[] = [];
    const create = (resume?: string) => {
      const host = new Host({ cwd, resume }, (event) => events.push(event), factory);
      hosts.push(host);
      return host;
    };
    async function finish(host: Host, id: string, text: string, expected = "fixture answer") {
      host.submit(id, text);
      const deadline = Date.now() + 20000;
      while (!events.some((event) => event.kind === "finished" && event.input_id === id)) {
        assert.equal(host.status().unavailable, null);
        assert.ok(Date.now() < deadline, "SDK did not finish the fixture task");
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      assert.deepEqual(
        events.flatMap((e) => (e.kind === "finished" && e.input_id === id ? [e.outcome] : [])),
        ["completed"],
      );
      assert.equal(events.filter((e) => e.kind === "started" && e.input_id === id).length, 1);
      assert.equal(
        events.flatMap((e) => (e.kind === "text" && e.input_id === id ? [e.delta] : [])).join(""),
        expected,
      );
    }
    try {
      const host = create();
      await finish(host, randomUUID(), "First isolated task", "readingfixture answer");
      assert.equal(
        events.filter((event) => event.kind === "tool" && event.call_id === "read-fixture").length,
        1,
      );
      const toolResults = events.filter((event) => event.kind === "tool_result");
      assert.equal(toolResults.length, 1);
      assert.equal(toolResults[0]!.failed, false);
      assert.ok(JSON.stringify(toolResults[0]!.content).includes("isolated tool result marker"));
      const native = host.status().native_session_id;
      assert.ok(native);
      await finish(host, randomUUID(), "Second isolated task");
      await host.close();
      assert.equal(host.status().closed, true);
      const restored = create(native);
      await finish(restored, randomUUID(), "Third isolated task");
      assert.equal(restored.status().native_session_id, native);
      await restored.close();
      assert.equal(requests.length, 4);
      const resumedHistory = JSON.stringify(requests[3]);
      assert.ok(resumedHistory.includes("First isolated task"));
      assert.ok(resumedHistory.includes("Second isolated task"));
    } finally {
      try {
        await Promise.all(hosts.map((host) => host.close()));
      } finally {
        server.closeAllConnections();
        await new Promise<void>((resolve) => server.close(() => resolve()));
      }
    }
  },
);
