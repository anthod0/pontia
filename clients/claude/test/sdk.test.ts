import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdir, mkdtempDisposable, writeFile, readFile, access, readdir } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { spawn } from "node:child_process";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { listModels } from "../src/models.js";
import { Host, type QueryFactory, type ExecutionPolicy } from "../src/host.js";
import type { HostEvent } from "../src/facts.js";

// Opt-in runs the real SDK against a loopback-only model substitute, never a live model.
test(
  "real SDK: sequential tasks, tool approval, and session environment across resume",
  { skip: process.env.PONTIA_CLAUDE_SDK_TEST !== "1", timeout: 60000 },
  async () => {
    await using sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-host-sdk-"));
    const testRoot = sandbox.path;
    const cwd = join(testRoot, "workspace");
    const config = join(testRoot, "config");
    const temporary = join(testRoot, "tmp");
    await Promise.all([cwd, config, temporary].map((path) => mkdir(path)));
    await mkdir(join(cwd, ".claude"));
    for (const path of [
      join(config, "settings.json"),
      join(cwd, ".claude", "settings.json"),
      join(cwd, ".claude", "settings.local.json"),
    ]) {
      await writeFile(
        path,
        JSON.stringify({ env: { PONTIA_SESSION_ID: "wrong-settings-session" } }),
      );
    }
    await writeFile(join(cwd, "input.txt"), "isolated tool result marker\n");
    const requests: Array<{ messages: unknown; model: string; tools: Array<{ name: string }> }> = [];
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
        requests.push({ messages: body.messages, model: body.model, tools: body.tools ?? [] });
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
              : [3, 5, 11, 13].includes(requests.length)
                ? [
                    {
                      type: "tool_use",
                      id: `write-${requests.length}`,
                      name: "Write",
                      input: {
                        file_path: join(cwd, `write-${requests.length}.txt`),
                        content: "approved write marker",
                      },
                    },
                  ]
                : [7, 9].includes(requests.length)
                  ? [
                      {
                        type: "tool_use",
                        id: `environment-${requests.length}`,
                        name: "Bash",
                        input: { command: "printenv PONTIA_SESSION_ID" },
                      },
                    ]
                  : [{ type: "text", text: "fixture answer" }],
          stop_reason: [1, 3, 5, 7, 9, 11, 13].includes(requests.length) ? "tool_use" : "end_turn",
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
                ? { type: "text_delta", text: "text" in block ? block.text : "" }
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
      PONTIA_SESSION_ID: "wrong-inherited-session",
    });
    const factory: QueryFactory = (args) =>
      query({
        ...args,
        options: {
          ...args.options,
          env,
          strictMcpConfig: true,
          mcpServers: {},
          plugins: [],
          model: args.options?.model ?? "claude-sonnet-4-6",
          maxTurns: 4,
        },
      });
    const hosts: Host[] = [];
    const events: HostEvent[] = [];
    async function history(session: string) {
      const child = spawn(
        process.execPath,
        [new URL("../src/main.js", import.meta.url).pathname, "history"],
        { env, stdio: ["pipe", "pipe", "ignore"] },
      );
      child.stdin.end(JSON.stringify({ session_id: session, cwd }) + "\n");
      let output = "";
      child.stdout.on("data", (chunk) => {
        output += chunk;
      });
      const code = await new Promise<number | null>((resolve) => child.on("close", resolve));
      assert.equal(code, 0);
      return JSON.parse(output) as Array<{ uuid: string; session_id: string; message: unknown }>;
    }
    const create = (resume?: string, execution_policy?: ExecutionPolicy) => {
      const host = new Host(
        { session_id: "pontia-session", cwd, resume, model: "haiku", execution_policy },
        (event) => events.push(event),
        factory,
      );
      hosts.push(host);
      return host;
    };
    async function finish(
      host: Host,
      id: string,
      text: string,
      expected = "fixture answer",
      choice?: "allow" | "deny",
    ) {
      const handled = new Set<string>();
      host.submit(id, text);
      const deadline = Date.now() + 20000;
      while (!events.some((event) => event.kind === "finished" && event.input_id === id)) {
        assert.equal(
          host.status().unavailable,
          null,
          JSON.stringify(events.filter((e) => e.kind === "unavailable")),
        );
        assert.ok(Date.now() < deadline, "SDK did not finish the fixture task");
        for (const event of events) {
          if (
            event.kind === "permission_requested" &&
            !handled.has(event.permission_id) &&
            !events.some(
              (e) => e.kind === "permission_settled" && e.permission_id === event.permission_id,
            )
          ) {
            assert.ok(choice, "unexpected SDK approval");
            handled.add(event.permission_id);
            host.resolvePermission(event.permission_id, choice);
          }
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      assert.equal(handled.size, choice ? 1 : 0);
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
      const availableModels = await listModels(cwd, factory);
      assert.ok(availableModels.length > 0);
      assert.equal(requests.length, 0, "model listing must not run a task");
      const host = create();
      await finish(host, randomUUID(), "First isolated task", "readingfixture answer");
      assert.equal(
        events.filter((event) => event.kind === "tool" && event.call_id === "read-fixture").length,
        1,
      );
      const usageDeadline = Date.now() + 5000;
      while (!events.some((event) => event.kind === "context_usage")) {
        assert.ok(Date.now() < usageDeadline, "real SDK did not report context usage");
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      const context = events.find((event) => event.kind === "context_usage")!.context_usage;
      assert.ok(context.used_tokens > 0);
      assert.ok(context.max_tokens > 0);
      const toolResults = events.filter((event) => event.kind === "tool_result");
      assert.equal(toolResults.length, 1);
      assert.equal(toolResults[0]!.failed, false);
      assert.ok(JSON.stringify(toolResults[0]!.content).includes("isolated tool result marker"));
      const native = host.status().native_session_id;
      assert.ok(native);
      await finish(host, randomUUID(), "Second isolated task", "fixture answer", "allow");
      assert.ok(requests[2]!.model.includes("haiku"));
      assert.ok(requests[0]!.model.includes("haiku"));
      assert.equal(host.status().native_session_id, native);
      assert.equal(await readFile(join(cwd, "write-3.txt"), "utf8"), "approved write marker");
      await finish(host, randomUUID(), "Denied isolated task", "fixture answer", "deny");
      await assert.rejects(access(join(cwd, "write-5.txt")));
      assert.ok(
        events.some((e) => e.kind === "tool_result" && e.call_id === "write-5" && e.failed),
      );
      await finish(host, randomUUID(), "Read Session environment", "fixture answer", "allow");
      await host.close();
      assert.equal(host.status().closed, true);
      const before = await history(native);
      for (const event of events) {
        if (event.kind === "finished") {
          assert.ok(
            before.some((m) => m.uuid === event.input_id),
            "native history preserves input UUID",
          );
          assert.ok(
            before.some((m) => m.uuid === event.history_anchor),
            "native history preserves last message UUID",
          );
        }
      }
      const restored = create(native);
      await finish(
        restored,
        randomUUID(),
        "Read resumed Session environment",
        "fixture answer",
        "allow",
      );
      assert.equal(restored.status().native_session_id, native);
      await restored.close();
      assert.equal(requests.length, 10);
      const environmentResults = events.filter(
        (event): event is Extract<HostEvent, { kind: "tool_result" }> =>
          event.kind === "tool_result" && event.call_id.startsWith("environment-"),
      );
      assert.equal(environmentResults.length, 2);
      for (const event of environmentResults) {
        assert.equal(event.failed, false);
        assert.ok(typeof event.content === "string");
        assert.equal(event.content.trim(), "pontia-session");
      }
      const resumedHistory = JSON.stringify(requests[8]);
      assert.ok(resumedHistory.includes("First isolated task"));
      assert.ok(resumedHistory.includes("Second isolated task"));
      const after = await history(native);
      assert.ok(after.length > before.length);
      assert.ok(after.every((m) => m.session_id === native));
      await assert.rejects(history(randomUUID()));
      const transcript = (await readdir(config, { recursive: true })).find((path) =>
        path.endsWith(`/${native}.jsonl`),
      );
      assert.ok(transcript);
      const path = join(config, transcript);
      const saved = await readFile(path);
      try {
        await writeFile(path, "invalid transcript\n");
        await assert.rejects(history(native));
      } finally {
        await writeFile(path, saved);
      }
      const policy: ExecutionPolicy = {
        tools: ["Read", "Bash", "Glob", "Write"],
        disabled_tools: ["Write", "Agent"], disable_mcp: true, yolo: true,
      };
      const restricted = create(undefined, policy);
      await finish(restricted, randomUUID(), "Denied tool under YOLO");
      const restrictedNative = restricted.status().native_session_id!;
      await restricted.close();
      const resumedRestricted = create(restrictedNative, policy);
      await finish(resumedRestricted, randomUUID(), "Denied tool after resume under YOLO");
      for (const index of [10, 12]) {
        const names = requests[index]!.tools.map((tool) => tool.name);
        assert.ok(names.includes("Glob"));
        assert.ok(!names.includes("Write"));
        assert.ok(!names.includes("Agent"));
        await assert.rejects(access(join(cwd, `write-${index + 1}.txt`)));
      }
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

for (const phase of ["streaming", "approval"] as const) {
  test(
    `real SDK: native steering during ${phase} continues the task and preserves both inputs in history`,
    {
      skip: process.env.PONTIA_CLAUDE_SDK_TEST !== "1",
      timeout: 60000,
    },
    async () => {
      await using sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-steer-sdk-"));
      const cwd = join(sandbox.path, "workspace");
      const config = join(sandbox.path, "config");
      const temporary = join(sandbox.path, "tmp");
      await Promise.all([cwd, config, temporary].map((path) => mkdir(path)));
      const requests: unknown[] = [];
      const events: HostEvent[] = [];
      const server = createServer(async (request, response) => {
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
        requests.push(body.messages);
        const message = {
          id: `steering_${requests.length}`,
          type: "message",
          role: "assistant",
          model: body.model,
          content: [],
          stop_reason: null,
          stop_sequence: null,
          usage: { input_tokens: 12, output_tokens: 1 },
        };
        response.writeHead(200, { "content-type": "text/event-stream" });
        const send = (frame: { type: string; [key: string]: unknown }) =>
          response.write(`event: ${frame.type}\ndata: ${JSON.stringify(frame)}\n\n`);
        send({ type: "message_start", message });
        if (requests.length === 1 && phase === "approval") {
          send({
            type: "content_block_start",
            index: 0,
            content_block: { type: "tool_use", id: "steer-write", name: "Write", input: {} },
          });
          send({
            type: "content_block_delta",
            index: 0,
            delta: {
              type: "input_json_delta",
              partial_json: JSON.stringify({
                file_path: join(cwd, "not-approved.txt"),
                content: "unapproved",
              }),
            },
          });
          send({ type: "content_block_stop", index: 0 });
          send({
            type: "message_delta",
            delta: { stop_reason: "tool_use", stop_sequence: null },
            usage: { output_tokens: 2 },
          });
          send({ type: "message_stop" });
          response.end();
          return;
        }
        send({ type: "content_block_start", index: 0, content_block: { type: "text", text: "" } });
        send({
          type: "content_block_delta",
          index: 0,
          delta: { type: "text_delta", text: requests.length === 1 ? "working" : "redirected" },
        });
        // Keep the original response streaming until native steering aborts it.
        if (requests.length === 1) return;
        send({ type: "content_block_stop", index: 0 });
        send({
          type: "message_delta",
          delta: { stop_reason: "end_turn", stop_sequence: null },
          usage: { output_tokens: 2 },
        });
        send({ type: "message_stop" });
        response.end();
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
      const host = new Host(
        { session_id: "steering-session", cwd },
        (event) => events.push(event),
        (args) =>
          query({
            ...args,
            options: {
              ...args.options,
              env,
              strictMcpConfig: true,
              mcpServers: {},
              plugins: [],
              model: "claude-sonnet-4-6",
            },
          }),
      );
      const first = randomUUID();
      const second = randomUUID();
      const waitFor = async (predicate: () => boolean) => {
        const deadline = Date.now() + 20000;
        while (!predicate()) {
          assert.equal(
            host.status().unavailable,
            null,
            JSON.stringify(events.filter((e) => e.kind === "unavailable")),
          );
          assert.ok(Date.now() < deadline, "SDK steering did not progress");
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
      };
      try {
        host.submit(first, "original steering fixture task");
        await waitFor(() =>
          events.some((e) => e.kind === (phase === "approval" ? "permission_requested" : "text")),
        );
        host.steer(first, second, "native steering correction marker");
        await waitFor(() => host.status().finished);
        assert.deepEqual(
          events.filter((e) => e.kind === "started").map((e) => e.input_id),
          [first],
        );
        assert.deepEqual(
          events.filter((e) => e.kind === "finished").map((e) => [e.input_id, e.outcome]),
          [[first, "completed"]],
        );
        assert.ok(
          events.some((e) => e.kind === "text" && e.delta === "redirected" && e.input_id === first),
        );
        assert.ok(JSON.stringify(requests[1]).includes("native steering correction marker"));
        await host.close();
        const native = host.status().native_session_id!;
        const transcript = (await readdir(config, { recursive: true })).find((path) =>
          path.endsWith(`/${native}.jsonl`),
        );
        assert.ok(transcript);
        const history = await readFile(join(config, transcript), "utf8");
        assert.ok(history.includes(first));
        assert.ok(history.includes(second));
        if (phase === "approval") {
          assert.ok(events.some((e) => e.kind === "permission_settled" && e.outcome === "expired"));
          await assert.rejects(access(join(cwd, "not-approved.txt")));
        }
      } finally {
        try {
          await host.close();
        } finally {
          server.closeAllConnections();
          await new Promise<void>((resolve) => server.close(() => resolve()));
        }
      }
    },
  );
}
