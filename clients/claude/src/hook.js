#!/usr/bin/env node

import { resolve } from "node:path";
import { stdin as processStdin, stdout as processStdout } from "node:process";
import { pathToFileURL } from "node:url";
import { appendDiagnostic } from "./diagnostics.js";
import { claimTurnContext, defaultHookLogFile, loadCurrentTurnByClientSession, loadSessionByClientSession } from "./context.js";
import { buildSessionExitedEvent, buildSessionReadyEvent, buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnOutputEvent, buildTurnStartedEvent } from "./events.js";
import { optionalString } from "./internal-api.js";
import { hasTmuxPaneEnvironment } from "./managed-runtime.js";
import { EventReporter } from "./reporter.js";
import { bindManualSession } from "./runtime-binding.js";
import { isActiveRegisteredWorkspace } from "./workspace.js";
import { resolvePontiaConnection } from "./discovery.js";
function sessionDetailsFromHook(input) {
    return {
        clientSessionKey: optionalString(input.session_id),
        transcriptPath: optionalString(input.transcript_path),
        clientCwd: optionalString(input.cwd),
    };
}
function activeTurnContext(context) {
    return context.turnId ? { ...context, turnId: context.turnId } : undefined;
}
function failureMessage(input) {
    const error = optionalString(input.error) ?? "Claude Code turn failed";
    const details = optionalString(input.error_details);
    return details ? `${error}: ${details}` : error;
}
async function reportReadyForClientSession(input, deps) {
    const details = sessionDetailsFromHook(input);
    const logFile = defaultHookLogFile(deps.env);
    const workspaceActive = await isActiveRegisteredWorkspace(deps.env, deps.fetchImpl, details.clientCwd);
    if (workspaceActive !== true) {
        await deps.logDiagnostic(logFile, {
            level: "info",
            code: workspaceActive === false ? "workspace_not_active" : "workspace_check_unavailable",
            message: workspaceActive === false
                ? "current claude workspace is not an active registered pontia workspace; pontia reporting disabled"
                : "could not verify active registered pontia workspace; pontia reporting disabled",
            details: { client_cwd: details.clientCwd },
        });
        return;
    }
    const existing = details.clientSessionKey
        ? await loadSessionByClientSession(deps.env, deps.fetchImpl, details.clientSessionKey)
        : undefined;
    if (existing?.ok && ["idle", "busy", "interrupted"].includes(existing.context.sessionState)) {
        await deps.logDiagnostic(logFile, {
            level: "info",
            code: "duplicate_active_client_session",
            message: "native Claude session is already bound to an active pontia session; duplicate TUI reporting disabled",
            details: {
                client_session_key: details.clientSessionKey,
                session_id: existing.context.sessionId,
                session_state: existing.context.sessionState,
            },
        });
        return;
    }
    let context;
    if (!existing || existing.ok || existing.reason === "session context not found") {
        context = await bindManualSession(deps.env, deps.fetchImpl, details);
    }
    else {
        await deps.logDiagnostic(logFile, { level: "warn", code: "session_context_lookup_failed", message: existing.reason });
    }
    if (!context)
        return;
    await deps.makeReporter(logFile).report(context, buildSessionReadyEvent(context));
}
async function handleSessionStart(input, deps) {
    await reportReadyForClientSession(input, deps);
}
async function handleSessionEnd(input, deps) {
    const clientSessionKey = optionalString(input.session_id);
    if (!clientSessionKey)
        return;
    const loaded = await loadSessionByClientSession(deps.env, deps.fetchImpl, clientSessionKey);
    if (!loaded.ok) {
        await deps.logDiagnostic(loaded.logFile, { level: "warn", code: "session_context_lookup_failed", message: loaded.reason });
        return;
    }
    await deps.makeReporter(loaded.logFile).report(loaded.context, buildSessionExitedEvent(loaded.context, optionalString(input.reason) ?? "exit"));
}
async function manualTurnContext(input, deps, logFile) {
    const details = sessionDetailsFromHook(input);
    const existing = details.clientSessionKey
        ? await loadSessionByClientSession(deps.env, deps.fetchImpl, details.clientSessionKey)
        : undefined;
    let session;
    if (existing?.ok && existing.context.sessionState !== "exited") {
        session = { ...existing.context, ...details };
    }
    else if (existing?.ok || existing?.reason === "session context not found") {
        const workspaceActive = await isActiveRegisteredWorkspace(deps.env, deps.fetchImpl, details.clientCwd);
        if (workspaceActive !== true) {
            await deps.logDiagnostic(logFile, {
                level: "info",
                code: workspaceActive === false ? "workspace_not_active" : "workspace_check_unavailable",
                message: workspaceActive === false
                    ? "current claude workspace is not an active registered pontia workspace; pontia reporting disabled"
                    : "could not verify active registered pontia workspace; pontia reporting disabled",
                details: { client_cwd: details.clientCwd },
            });
            return undefined;
        }
        session = await bindManualSession(deps.env, deps.fetchImpl, details);
        if (session)
            await deps.makeReporter(logFile).report(session, buildSessionReadyEvent(session));
    }
    else if (existing) {
        await deps.logDiagnostic(logFile, { level: "warn", code: "session_context_lookup_failed", message: existing.reason });
    }
    if (!session)
        return undefined;
    return {
        sessionId: session.sessionId,
        runtimeInstanceId: session.runtimeInstanceId,
        clientType: "claude",
        internalEventUrl: session.internalEventUrl,
        input: optionalString(input.prompt),
    };
}
async function handleUserPromptSubmit(input, deps) {
    const prompt = optionalString(input.prompt);
    const logFile = defaultHookLogFile(deps.env);
    const session = await manualTurnContext(input, deps, logFile);
    if (!session)
        return;
    const claimed = await claimTurnContext(deps.env, deps.fetchImpl, session);
    if (!claimed.ok && !claimed.silent) {
        await deps.logDiagnostic(logFile, {
            level: "warn",
            code: "current_turn_claim_failed",
            message: claimed.reason,
        });
        return;
    }
    const context = claimed.ok
        ? { ...claimed.context, input: prompt ?? claimed.context.input }
        : { ...session, input: prompt };
    const result = await deps.makeReporter(logFile).report(context, buildTurnStartedEvent(context));
    if (result.accepted && !result.turnId) {
        await deps.logDiagnostic(logFile, {
            level: "error",
            code: "turn_start_not_normalized",
            message: "pontia did not return a canonical turn_id for turn.started",
        });
    }
}
async function handleStop(input, deps) {
    const clientSessionKey = optionalString(input.session_id);
    if (!clientSessionKey)
        return;
    const session = await loadSessionByClientSession(deps.env, deps.fetchImpl, clientSessionKey);
    if (!session.ok)
        return;
    const loaded = await loadCurrentTurnByClientSession(deps.env, deps.fetchImpl, clientSessionKey, session.context);
    if (!loaded.ok) {
        if (!loaded.silent)
            await deps.logDiagnostic(loaded.logFile, { level: "warn", code: "current_turn_lookup_failed", message: loaded.reason });
        return;
    }
    const context = activeTurnContext(loaded.context);
    if (!context) {
        await deps.logDiagnostic(loaded.logFile, {
            level: "error",
            code: "current_turn_missing_turn_id",
            message: "pontia current-turn lookup did not return a canonical turn_id",
        });
        return;
    }
    const reporter = deps.makeReporter(loaded.logFile);
    const output = optionalString(input.last_assistant_message);
    if (output) {
        const result = await reporter.report(context, buildTurnOutputEvent(context, output));
        if (!result.accepted)
            return;
    }
    await reporter.report(context, buildTurnCompletedEvent(context));
}
async function handleStopFailure(input, deps) {
    const clientSessionKey = optionalString(input.session_id);
    if (!clientSessionKey)
        return;
    const session = await loadSessionByClientSession(deps.env, deps.fetchImpl, clientSessionKey);
    if (!session.ok)
        return;
    const loaded = await loadCurrentTurnByClientSession(deps.env, deps.fetchImpl, clientSessionKey, session.context);
    if (!loaded.ok)
        return;
    const context = activeTurnContext(loaded.context);
    if (!context) {
        await deps.logDiagnostic(loaded.logFile, {
            level: "error",
            code: "current_turn_missing_turn_id",
            message: "pontia current-turn lookup did not return a canonical turn_id",
        });
        return;
    }
    await deps.makeReporter(loaded.logFile).report(context, buildTurnFailedEvent(context, failureMessage(input)));
}
function permissionRequestUrl(internalEventUrl) {
    try {
        const url = new URL(internalEventUrl);
        url.pathname = url.pathname.replace(/\/events\/?$/, "/claude/permission-request");
        return url.toString();
    }
    catch {
        return undefined;
    }
}
function permissionDecisionOutput(result) {
    if (!result || typeof result !== "object" || Array.isArray(result))
        return undefined;
    if (result.decision === "accept_once") {
        return {
            hookSpecificOutput: {
                hookEventName: "PermissionRequest",
                decision: { behavior: "allow" },
            },
        };
    }
    if (result.decision === "reject") {
        return {
            hookSpecificOutput: {
                hookEventName: "PermissionRequest",
                decision: { behavior: "deny" },
            },
        };
    }
    const suggestion = result.permission_suggestion;
    if (result.decision !== "always_allow" || !suggestion || typeof suggestion !== "object" || Array.isArray(suggestion))
        return undefined;
    return {
        hookSpecificOutput: {
            hookEventName: "PermissionRequest",
            decision: {
                behavior: "allow",
                updatedPermissions: [suggestion],
            },
        },
    };
}
async function handlePermissionRequest(input, deps) {
    const clientSessionKey = optionalString(input.session_id);
    const toolName = optionalString(input.tool_name);
    if (!clientSessionKey || !toolName || !input.tool_input || typeof input.tool_input !== "object" || Array.isArray(input.tool_input))
        return;
    const connection = await resolvePontiaConnection({ env: deps.env, fetch: deps.fetchImpl });
    const url = connection?.internalEventUrl ? permissionRequestUrl(connection.internalEventUrl) : undefined;
    if (!url)
        return;
    try {
        const response = await deps.fetchImpl(url, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                session_id: clientSessionKey,
                prompt_id: optionalString(input.prompt_id),
                tool_name: toolName,
                tool_input: input.tool_input,
                permission_suggestions: Array.isArray(input.permission_suggestions)
                    ? input.permission_suggestions
                    : [],
                hook_input: input,
            }),
        });
        if (response.status === 204)
            return;
        if (!response.ok) {
            await deps.logDiagnostic(defaultHookLogFile(deps.env), {
                level: "warn",
                code: "permission_request_registration_failed",
                message: `Pontia PermissionRequest API returned ${response.status}`,
            });
            return;
        }
        const body = await response.json();
        return permissionDecisionOutput(body?.data?.result);
    }
    catch (error) {
        await deps.logDiagnostic(defaultHookLogFile(deps.env), {
            level: "warn",
            code: "permission_request_registration_exception",
            message: "Pontia PermissionRequest API could not be reached",
            details: error instanceof Error ? error.message : String(error),
        });
    }
}
function requiredDeps(dependencies) {
    const env = dependencies.env ?? process.env;
    const fetchImpl = dependencies.fetch ?? fetch;
    return {
        env,
        fetchImpl,
        makeReporter: dependencies.makeReporter ?? ((logFile) => new EventReporter({ logFile, fetch: fetchImpl })),
        logDiagnostic: dependencies.logDiagnostic ?? appendDiagnostic,
    };
}
export async function runClaudeHook(input, dependencies = {}) {
    const deps = requiredDeps(dependencies);
    if (!hasTmuxPaneEnvironment(deps.env))
        return;
    try {
        switch (input.hook_event_name) {
            case "SessionStart":
                await handleSessionStart(input, deps);
                break;
            case "UserPromptSubmit":
                await handleUserPromptSubmit(input, deps);
                break;
            case "Stop":
                await handleStop(input, deps);
                break;
            case "StopFailure":
                await handleStopFailure(input, deps);
                break;
            case "PermissionRequest":
                return await handlePermissionRequest(input, deps);
            case "SessionEnd":
                await handleSessionEnd(input, deps);
                break;
            default:
                break;
        }
    }
    catch (error) {
        await deps.logDiagnostic(defaultHookLogFile(deps.env), {
            level: "error",
            code: "unexpected_hook_exception",
            message: "failed to process Claude Code pontia hook",
            details: error instanceof Error ? error.message : String(error),
        });
    }
}
async function readStdin() {
    let data = "";
    for await (const chunk of processStdin)
        data += chunk;
    return data;
}
export async function main() {
    const text = await readStdin();
    if (!text.trim())
        return;
    try {
        const output = await runClaudeHook(JSON.parse(text));
        if (output)
            processStdout.write(JSON.stringify(output));
    }
    catch {
        // Hook must no-op on malformed input.
    }
}
const invokedPath = process.argv[1];
if (invokedPath && import.meta.url === pathToFileURL(resolve(invokedPath)).href) {
    void main();
}
