const MAX_TURN_OUTPUT_CHARS = 200;
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
        data.transcript_path = context.transcriptPath;
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
export function buildTurnCompletedEvent(context) {
    return turnFact(context, "turn.completed", {});
}
export function buildTurnFailedEvent(context, message) {
    return turnFact(context, "turn.failed", { failure: { message } });
}
