import { describe, expect, test } from "vitest";
import { buildToolTimelineEvent, buildTurnOutputEvent, buildTurnStartedEvent } from "../src/events.js";
const context = {
    sessionId: "sess_1",
    turnId: "turn_1",
    runtimeInstanceId: "rtinst_1",
    clientType: "claude",
    internalEventUrl: "http://127.0.0.1:8080/internal/v1/events",
};
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
    test("maps Read tool input to a bounded structured timeline item", () => {
        const event = buildToolTimelineEvent(context, {
            hook_event_name: "PreToolUse",
            tool_use_id: "toolu_1",
            tool_name: "Read",
            tool_input: { file_path: `/repo/${"界".repeat(400)}.rs`, offset: 4, limit: 3 },
        });
        expect(event).toEqual({
            session_id: "sess_1",
            turn_id: "turn_1",
            type: "turn.timeline_item",
            data: {
                item_id: "claude:toolu_1:call",
                kind: "tool_call",
                raw_kind: "PreToolUse",
                role: "assistant",
                title: "Read file",
                status: "started",
                occurred_at: null,
                content_preview: expect.stringMatching(/^\/repo\//),
                managed_tool_use: {
                    tool_name: "Read",
                    input: { type: "read", path: expect.any(String), start_line: 4, end_line: 6 },
                },
            },
        });
        expect(event.data.content_preview.length).toBeLessThanOrEqual(300);
        expect(event.data.managed_tool_use.input.path.length).toBeLessThanOrEqual(300);
    });
    test("summarizes Edit, Write, and Bash without retaining file contents or obvious secrets", () => {
        const edit = buildToolTimelineEvent(context, {
            hook_event_name: "PreToolUse",
            tool_use_id: "edit_1",
            tool_name: "Edit",
            tool_input: { file_path: "/repo/secret.txt", old_string: "private old", new_string: "private new" },
        });
        const write = buildToolTimelineEvent(context, {
            hook_event_name: "PreToolUse",
            tool_use_id: "write_1",
            tool_name: "Write",
            tool_input: { file_path: "/repo/output.txt", content: "private file contents" },
        });
        const bash = buildToolTimelineEvent(context, {
            hook_event_name: "PreToolUse",
            tool_use_id: "bash_1",
            tool_name: "Bash",
            tool_input: { command: `API_TOKEN=super-secret curl --password hunter2 ${"x".repeat(400)}`, timeout: 120000 },
        });

        expect(edit.data.managed_tool_use).toEqual({ tool_name: "Edit", input: { type: "edit", path: "/repo/secret.txt", edits_count: 1 } });
        expect(write.data.managed_tool_use).toEqual({ tool_name: "Write", input: { type: "write", path: "/repo/output.txt" } });
        expect(JSON.stringify([edit, write])).not.toContain("private");
        expect(bash.data.managed_tool_use.input).toEqual({
            type: "bash",
            command: expect.stringContaining("API_TOKEN=<redacted>"),
            timeout: 120000,
        });
        expect(bash.data.managed_tool_use.input.command).toContain("--password <redacted>");
        expect(bash.data.managed_tool_use.input.command).not.toContain("super-secret");
        expect(bash.data.managed_tool_use.input.command).not.toContain("hunter2");
        expect(Array.from(bash.data.managed_tool_use.input.command).length).toBeLessThanOrEqual(300);
    });
    test("tool results and unknown tools omit raw input, responses, and errors", () => {
        const unknown = buildToolTimelineEvent(context, {
            hook_event_name: "PreToolUse",
            tool_use_id: "unknown_1",
            tool_name: "mcp__private__lookup",
            tool_input: { query: "sensitive query" },
        });
        const completed = buildToolTimelineEvent(context, {
            hook_event_name: "PostToolUse",
            tool_use_id: "unknown_1",
            tool_name: "mcp__private__lookup",
            tool_response: { result: "sensitive response" },
            duration_ms: 42,
        });
        const failed = buildToolTimelineEvent(context, {
            hook_event_name: "PostToolUseFailure",
            tool_use_id: "failed_1",
            tool_name: "Bash",
            error: "sensitive failure details",
            duration_ms: 7,
        });

        expect(unknown.data).toMatchObject({ kind: "tool_call", title: "mcp__private__lookup", content_preview: "mcp__private__lookup" });
        expect(unknown.data).not.toHaveProperty("managed_tool_use");
        expect(completed.data).toMatchObject({ kind: "tool_result", status: "completed", content_preview: "mcp__private__lookup completed · 42 ms" });
        expect(failed.data).toMatchObject({ kind: "tool_result", status: "error", content_preview: "Run command failed · 7 ms" });
        expect(JSON.stringify([unknown, completed, failed])).not.toContain("sensitive");
    });
});
