import type { EventView, SessionView } from "../../api/types";

export function compactionDisabledReason(
  session: SessionView,
  pending: boolean,
): string | undefined {
  if (pending) return "Context compaction is already in progress";
  if (!session.capabilities.manual_compaction)
    return "Context compaction is unavailable for this session";
  if (session.state !== "idle" && session.state !== "interrupted")
    return "Wait for the session to become idle before compacting";
}

export function compactionStatus(events: EventView[], requestedOperation: string | null) {
  const operations = new Map<string, EventView>();
  for (const event of events) {
    if (
      ![
        "session.compaction_started",
        "session.compaction_completed",
        "session.compaction_failed",
      ].includes(event.type)
    )
      continue;
    const id = event.payload.operation_id as string;
    const previous = operations.get(id);
    if (!previous || event.type !== "session.compaction_started") operations.set(id, event);
  }
  if (requestedOperation && !operations.has(requestedOperation)) {
    return {
      phase: "accepted",
      message: "Compaction requested. Waiting for the client…",
      busy: true,
    };
  }
  const event = [...operations.values()]
    .sort((a, b) => a.time.localeCompare(b.time) || a.event_id.localeCompare(b.event_id))
    .at(-1);
  if (!event) return null;
  if (event.type === "session.compaction_started")
    return { phase: "running", message: "Compacting context…", busy: true };
  if (event.type === "session.compaction_failed") {
    return {
      phase: "failed",
      message:
        (event.payload.error_message as string | null) ||
        (event.payload.aborted
          ? "Context compaction was cancelled."
          : "Context compaction failed."),
      busy: false,
    };
  }
  const before = event.payload.tokens_before;
  const after = event.payload.tokens_after;
  const usage =
    typeof before === "number" && typeof after === "number"
      ? ` (${before.toLocaleString()} → ${after.toLocaleString()} tokens)`
      : "";
  return { phase: "completed", message: `Context compacted${usage}`, busy: false };
}
