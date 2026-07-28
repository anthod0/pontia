import { randomUUID } from "node:crypto";

const MAX_TURN_OUTPUT_CHARS = 200;
const MAX_TOOL_TEXT_CHARS = 300;
function boundedString(value, limit = MAX_TOOL_TEXT_CHARS) {
    return typeof value === "string" ? Array.from(value).slice(0, limit).join("") : undefined;
}
function nonNegativeInteger(value) {
    return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
}
function safeCommand(value) {
    if (typeof value !== "string")
        return "command details omitted";
    const redacted = value
        .replace(/\b([A-Za-z_][A-Za-z0-9_]*(?:TOKEN|SECRET|PASSWORD|PASSWD|API_KEY|PRIVATE_KEY)[A-Za-z0-9_]*)=(?:"[^"]*"|'[^']*'|[^\s]+)/gi, "$1=<redacted>")
        .replace(/((?:--?|\/)(?:token|secret|password|passwd|api-key|authorization)\s+)(?:"[^"]*"|'[^']*'|[^\s]+)/gi, "$1<redacted>")
        .replace(/((?:--?|\/)(?:token|secret|password|passwd|api-key|authorization)=)(?:"[^"]*"|'[^']*'|[^\s]+)/gi, "$1<redacted>")
        .replace(/\bBearer\s+[^\s]+/gi, "Bearer <redacted>");
    return boundedString(redacted) ?? "command details omitted";
}
function toolLabel(toolName) {
    switch (toolName) {
        case "Read": return "Read file";
        case "Edit": return "Edit file";
        case "Write": return "Write file";
        case "Bash": return "Run command";
        default: return toolName;
    }
}
function turnFact(context, type, data) {
    return {
        session_id: context.sessionId,
        turn_id: context.turnId,
        type,
        data,
    };
}
export function buildSessionReadyEvent(context) {
    const data = { runtime_instance_id: context.runtimeInstanceId };
    if (context.clientSessionKey)
        data.client_session_key = context.clientSessionKey;
    if (context.transcriptPath)
        data.client_session_file = context.transcriptPath;
    if (context.clientCwd)
        data.client_cwd = context.clientCwd;
    return {
        session_id: context.sessionId,
        type: "session.ready",
        data,
    };
}
export function buildSessionExitedEvent(context, reason) {
    return {
        session_id: context.sessionId,
        type: "session.exited",
        data: { reason, runtime_instance_id: context.runtimeInstanceId },
    };
}
export function buildTurnStartedEvent(context) {
    return {
        session_id: context.sessionId,
        type: "turn.started",
        data: {
            runtime_instance_id: context.runtimeInstanceId,
            input: context.input ? { summary: context.input } : {},
        },
    };
}
export function buildTurnOutputEvent(context, output) {
    return turnFact(context, "turn.output", {
        output: { summary: Array.from(output).slice(0, MAX_TURN_OUTPUT_CHARS).join("") },
    });
}
export function buildToolTimelineEvent(context, input) {
    const toolName = boundedString(input.tool_name, 100) ?? "Tool";
    const toolUseId = boundedString(input.tool_use_id, 200) ?? randomUUID();
    const toolInput = input.tool_input && typeof input.tool_input === "object" && !Array.isArray(input.tool_input)
        ? input.tool_input
        : {};
    let title = toolLabel(toolName);
    let contentPreview = title;
    let managedToolUse;
    if (input.hook_event_name === "PreToolUse") {
        const path = boundedString(toolInput.file_path ?? toolInput.path) ?? "unknown path";
        if (toolName === "Read") {
            const startLine = nonNegativeInteger(toolInput.offset);
            const limit = nonNegativeInteger(toolInput.limit);
            title = "Read file";
            contentPreview = path;
            managedToolUse = {
                tool_name: toolName,
                input: {
                    type: "read",
                    path,
                    ...(startLine !== undefined ? { start_line: startLine } : {}),
                    ...(startLine !== undefined && limit !== undefined && limit > 0
                        ? { end_line: startLine + limit - 1 }
                        : {}),
                },
            };
        }
        else if (toolName === "Edit") {
            title = "Edit file";
            contentPreview = path;
            managedToolUse = { tool_name: toolName, input: { type: "edit", path, edits_count: 1 } };
        }
        else if (toolName === "Write") {
            title = "Write file";
            contentPreview = path;
            managedToolUse = { tool_name: toolName, input: { type: "write", path } };
        }
        else if (toolName === "Bash") {
            const command = safeCommand(toolInput.command);
            const timeout = nonNegativeInteger(toolInput.timeout);
            title = "Run command";
            contentPreview = command;
            managedToolUse = {
                tool_name: toolName,
                input: { type: "bash", command, ...(timeout !== undefined ? { timeout } : {}) },
            };
        }
    }
    const status = input.hook_event_name === "PreToolUse"
        ? "started"
        : input.hook_event_name === "PostToolUseFailure" ? "error" : "completed";
    if (input.hook_event_name !== "PreToolUse") {
        const outcome = status === "error" ? "failed" : "completed";
        const duration = nonNegativeInteger(input.duration_ms);
        title = `${toolLabel(toolName)} ${outcome}`;
        contentPreview = `${title}${duration !== undefined ? ` · ${duration} ms` : ""}`;
    }
    return turnFact(context, "turn.timeline_item", {
        item_id: `claude:${toolUseId}:${input.hook_event_name === "PreToolUse" ? "call" : "result"}`,
        kind: input.hook_event_name === "PreToolUse" ? "tool_call" : "tool_result",
        raw_kind: input.hook_event_name,
        role: "assistant",
        title,
        status,
        occurred_at: null,
        content_preview: contentPreview,
        ...(managedToolUse ? { managed_tool_use: managedToolUse } : {}),
    });
}
export function buildTurnCompletedEvent(context) {
    return turnFact(context, "turn.completed", {});
}
export function buildTurnFailedEvent(context, message) {
    return turnFact(context, "turn.failed", { failure: { message } });
}
