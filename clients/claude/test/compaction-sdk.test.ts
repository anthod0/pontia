import assert from "node:assert/strict";
import { test } from "node:test";
import { randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { mkdir, mkdtempDisposable } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { Host } from "../src/host.js";
import { Compaction } from "../src/compaction.js";
import type { HostEvent } from "../src/facts.js";

for (const automatic of [false, true]) {
  test(
    `real SDK: ${automatic ? "automatic" : "manual"} compaction and resumed history`,
    {
      skip: process.env.PONTIA_CLAUDE_SDK_TEST !== "1",
      timeout: 60000,
    },
    async () => {
      await using sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-compact-sdk-"));
      const cwd = join(sandbox.path, "workspace"),
        config = join(sandbox.path, "config");
      await Promise.all([mkdir(cwd), mkdir(config)]);
      let requests = 0;
      const server = createServer(async (request, response) => {
        const chunks: Buffer[] = [];
        for await (const chunk of request) chunks.push(Buffer.from(chunk));
        const body = JSON.parse(Buffer.concat(chunks).toString());
        if (request.url?.includes("count_tokens")) {
          response.end(JSON.stringify({ input_tokens: 200 }));
          return;
        }
        requests++;
        const text = "Preserve the fixture marker. ".repeat(100);
        const message = {
          id: `msg_${requests}`,
          type: "message",
          role: "assistant",
          model: body.model,
          content: [{ type: "text", text }],
          stop_reason: "end_turn",
          stop_sequence: null,
          usage: { input_tokens: automatic && requests === 1 ? 190000 : 2000, output_tokens: 200 },
        };
        if (!body.stream) {
          response.end(JSON.stringify(message));
          return;
        }
        response.writeHead(200, { "content-type": "text/event-stream" });
        const send = (value: { type: string; [key: string]: unknown }) =>
          response.write(`event: ${value.type}\ndata: ${JSON.stringify(value)}\n\n`);
        send({ type: "message_start", message: { ...message, content: [], stop_reason: null } });
        send({ type: "content_block_start", index: 0, content_block: { type: "text", text: "" } });
        send({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text } });
        send({ type: "content_block_stop", index: 0 });
        send({
          type: "message_delta",
          delta: { stop_reason: "end_turn", stop_sequence: null },
          usage: { output_tokens: 200 },
        });
        send({ type: "message_stop" });
        response.end();
      });
      await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
      const address = server.address();
      assert.ok(address && typeof address !== "string");
      const env = Object.fromEntries(
        ["HOME", "USERPROFILE", "PATH", "LANG", "LC_ALL", "SystemRoot"].map((key) => [
          key,
          process.env[key],
        ]),
      );
      Object.assign(env, {
        CLAUDE_CONFIG_DIR: config,
        TMPDIR: sandbox.path,
        ANTHROPIC_API_KEY: "fixture-not-a-secret",
        ANTHROPIC_BASE_URL: `http://127.0.0.1:${address.port}`,
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
        DISABLE_AUTOUPDATER: "1",
      });
      const events: HostEvent[] = [];
      const hosts: Host[] = [];
      const create = (resume?: string) => {
        const host = new Host(
          { session_id: "compact-sdk", cwd, resume },
          (event) => events.push(event),
          (args) =>
            query({
              ...args,
              options: {
                ...args.options,
                env,
                tools: [],
                settingSources: [],
                strictMcpConfig: true,
                mcpServers: {},
                model: "claude-sonnet-4-6",
              },
            }),
        );
        hosts.push(host);
        return host;
      };
      const wait = async (host: Host, predicate: () => boolean) => {
        const deadline = Date.now() + 20000;
        while (!predicate()) {
          assert.equal(host.status().unavailable, null);
          assert.ok(
            Date.now() < deadline,
            JSON.stringify(events.filter((e) => e.kind === "compaction")),
          );
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
      };
      try {
        if (!automatic) {
          const emptyFacts: HostEvent[] = [];
          const compaction = new Compaction((event) => emptyFacts.push(event));
          const input = compaction.request("empty-sdk");
          const empty = query({
            prompt: (async function* () {
              yield {
                type: "user" as const,
                uuid: input as ReturnType<typeof randomUUID>,
                session_id: "",
                parent_tool_use_id: null,
                message: { role: "user" as const, content: "/compact" },
              };
            })(),
            options: {
              cwd,
              env,
              tools: [],
              settingSources: [],
              strictMcpConfig: true,
              mcpServers: {},
            },
          });
          try {
            for await (const message of empty) compaction.accept(message);
          } finally {
            empty.close();
          }
          assert.deepEqual(
            emptyFacts.map((event) => event.kind === "compaction" && event.phase),
            ["started", "failed"],
          );
          assert.equal(compaction.busy, false);
          assert.equal(requests, 0);
        }
        const host = create();
        host.submit(randomUUID(), "Remember fixture marker. " + "context ".repeat(1000));
        await wait(host, () => host.status().finished);
        const native = host.status().native_session_id!;
        if (automatic) host.submit(randomUUID(), "Continue with the fixture marker.");
        else host.compact("manual-sdk");
        await wait(host, () =>
          events.some((e) => e.kind === "compaction" && e.phase === "completed"),
        );
        await wait(host, () => host.isIdle());
        const facts = events.filter((e) => e.kind === "compaction");
        assert.deepEqual(
          facts.map((e) => e.phase),
          ["started", "completed"],
        );
        assert.equal(facts[0]!.data.operation_id, facts[1]!.data.operation_id);
        assert.equal(facts[0]!.data.trigger, automatic ? "unknown" : "manual");
        assert.equal(events.filter((e) => e.kind === "started").length, automatic ? 2 : 1);
        assert.ok(facts[1]!.data.native_compaction_id);
        await host.close();
        const read = spawnSync(
          process.execPath,
          [new URL("../src/main.js", import.meta.url).pathname, "history"],
          {
            env,
            input: JSON.stringify({ session_id: native, cwd }) + "\n",
            encoding: "utf8",
          },
        );
        assert.equal(read.status, 0, read.stderr);
        const history = JSON.parse(read.stdout) as Array<Record<string, unknown>>;
        assert.ok(history.some((m) => m.isCompactSummary === true));
        const restored = create(native);
        restored.submit(randomUUID(), "Continue after resume.");
        await wait(restored, () => restored.status().finished);
        assert.equal(restored.status().native_session_id, native);
      } finally {
        await Promise.all(hosts.map((host) => host.close()));
        server.closeAllConnections();
        await new Promise<void>((resolve) => server.close(() => resolve()));
      }
    },
  );
}
