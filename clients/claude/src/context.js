import { homedir } from "node:os";
import { join } from "node:path";
import { resolvePontiaConnection } from "./discovery.js";
import { asRecord, optionalString } from "./internal-api.js";
function fallbackLogDir(env = process.env) {
    return join(env.PONTIA_HOME ?? join(env.HOME ?? homedir(), ".pontia"), "state");
}
export function defaultHookLogFile(env = process.env) {
    return join(fallbackLogDir(env), "claude-hook.log");
}
function claimUrl(internalEventUrl, sessionId) {
    try {
        const url = new URL(internalEventUrl);
        url.pathname = url.pathname.replace(/\/events\/?$/, `/sessions/${encodeURIComponent(sessionId)}/current-turn/claim`);
        return url.toString();
    }
    catch {
        return undefined;
    }
}
function agentBindingContextUrl(internalEventUrl, path, clientSessionKey) {
    try {
        const url = new URL(internalEventUrl);
        url.pathname = url.pathname.replace(/\/events\/?$/, `/agent-bindings/${path}`);
        url.searchParams.set("client_type", "claude");
        url.searchParams.set("client_session_key", clientSessionKey);
        return url.toString();
    }
    catch {
        return undefined;
    }
}
function contextMatchesRuntimeIdentity(context, runtimeIdentity) {
    return !runtimeIdentity
        || (context.sessionId === runtimeIdentity.sessionId
            && context.runtimeInstanceId === runtimeIdentity.runtimeInstanceId);
}
function contextFromRecord(record, logFile, internalEventUrl) {
    const sessionId = optionalString(record.session_id);
    const turnId = optionalString(record.turn_id);
    const clientType = optionalString(record.client_type);
    const runtimeInstanceId = optionalString(record.runtime_instance_id);
    const resolvedInternalEventUrl = optionalString(record.internal_event_url) ?? internalEventUrl;
    const errors = [];
    if (!sessionId)
        errors.push("session_id is required");
    if (clientType !== "claude")
        errors.push("client_type must be claude");
    if (!runtimeInstanceId)
        errors.push("runtime_instance_id is required");
    if (!resolvedInternalEventUrl)
        errors.push("internal_event_url is required");
    if (errors.length > 0)
        return { ok: false, reason: errors.join("; "), logFile };
    return {
        ok: true,
        logFile,
        context: {
            sessionId: sessionId,
            turnId,
            runtimeInstanceId: runtimeInstanceId,
            clientType: "claude",
            internalEventUrl: resolvedInternalEventUrl,
        },
    };
}
export async function claimTurnContext(env, fetchImpl, runtimeIdentity) {
    const logFile = defaultHookLogFile(env);
    if (!runtimeIdentity?.sessionId || !runtimeIdentity?.runtimeInstanceId)
        return { ok: false, reason: "managed runtime context unavailable", logFile, silent: true };
    const connection = await resolvePontiaConnection({ env, fetch: fetchImpl });
    const internalEventUrl = connection?.internalEventUrl;
    const url = internalEventUrl ? claimUrl(internalEventUrl, runtimeIdentity.sessionId) : undefined;
    if (!url)
        return { ok: false, reason: "current turn claim unavailable", logFile, silent: true };
    try {
        const response = await fetchImpl(url, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ runtime_instance_id: runtimeIdentity.runtimeInstanceId, client_type: "claude" }),
        });
        if (!response.ok)
            return { ok: false, reason: "current turn claim failed", logFile, silent: true };
        const body = await response.json();
        const currentTurn = asRecord(asRecord(asRecord(body)?.data)?.current_turn);
        if (!currentTurn)
            return { ok: false, reason: "no pending current turn", logFile, silent: true };
        const result = contextFromRecord(currentTurn, logFile, internalEventUrl);
        if (result.ok && !contextMatchesRuntimeIdentity(result.context, runtimeIdentity)) {
            return { ok: false, reason: "current turn context does not match tmux runtime identity", logFile };
        }
        return result;
    }
    catch {
        return { ok: false, reason: "current turn claim exception", logFile, silent: true };
    }
}
export async function loadSessionByClientSession(env, fetchImpl, clientSessionKey) {
    const logFile = defaultHookLogFile(env);
    const connection = await resolvePontiaConnection({ env, fetch: fetchImpl });
    const internalEventUrl = connection?.internalEventUrl;
    const url = internalEventUrl ? agentBindingContextUrl(internalEventUrl, "session-context", clientSessionKey) : undefined;
    if (!url)
        return { ok: false, reason: "session context lookup unavailable", logFile };
    try {
        const response = await fetchImpl(url);
        if (response.status === 404)
            return { ok: false, reason: "session context not found", logFile };
        if (!response.ok)
            return { ok: false, reason: "session context lookup failed", logFile };
        const body = await response.json();
        const record = asRecord(asRecord(asRecord(body)?.data)?.session_context);
        const sessionId = optionalString(record?.session_id);
        const sessionState = optionalString(record?.session_state);
        const runtimeInstanceId = optionalString(record?.runtime_instance_id);
        const clientType = optionalString(record?.client_type);
        const resolvedInternalEventUrl = optionalString(record?.internal_event_url) ?? internalEventUrl;
        if (!sessionId || !sessionState || !runtimeInstanceId || clientType !== "claude" || !resolvedInternalEventUrl) {
            return { ok: false, reason: "invalid session context lookup response", logFile };
        }
        return {
            ok: true,
            logFile,
            context: {
                sessionId,
                sessionState,
                runtimeInstanceId,
                clientType: "claude",
                internalEventUrl: resolvedInternalEventUrl,
                clientSessionKey,
            },
        };
    }
    catch {
        return { ok: false, reason: "session context lookup exception", logFile };
    }
}
export async function loadCurrentTurnByClientSession(env, fetchImpl, clientSessionKey, runtimeIdentity) {
    const logFile = defaultHookLogFile(env);
    const connection = await resolvePontiaConnection({ env, fetch: fetchImpl });
    const internalEventUrl = connection?.internalEventUrl;
    const url = internalEventUrl ? agentBindingContextUrl(internalEventUrl, "current-turn", clientSessionKey) : undefined;
    if (!url)
        return { ok: false, reason: "current turn lookup unavailable", logFile, silent: true };
    try {
        const response = await fetchImpl(url);
        if (!response.ok)
            return { ok: false, reason: "current turn lookup failed", logFile, silent: true };
        const body = await response.json();
        const currentTurn = asRecord(asRecord(asRecord(body)?.data)?.current_turn);
        if (!currentTurn)
            return { ok: false, reason: "no active current turn", logFile, silent: true };
        const result = contextFromRecord(currentTurn, logFile, internalEventUrl);
        if (result.ok && !contextMatchesRuntimeIdentity(result.context, runtimeIdentity))
            return { ok: false, reason: "current turn context does not match tmux runtime identity", logFile };
        return result;
    }
    catch {
        return { ok: false, reason: "current turn lookup exception", logFile, silent: true };
    }
}
