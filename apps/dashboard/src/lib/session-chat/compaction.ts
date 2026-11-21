import { translate as defaultTranslate, type Translator } from "$lib/i18n";
import type { EventView, SessionView, TurnTimelineItem } from "../../api/types";

export function compactionDisabledReason(
  session: SessionView,
  pending: boolean,
  translate: Translator = defaultTranslate,
): string | undefined {
  if (pending) return translate("compaction.inProgress");
  if (!session.capabilities.manual_compaction) return translate("compaction.unavailable");
  if (session.state !== "idle" && session.state !== "interrupted")
    return translate("compaction.waitIdle");
}

export function compactionStatus(
  events: EventView[],
  requestedOperation: string | null,
  translate: Translator = defaultTranslate,
) {
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
      message: translate("compaction.requested"),
      busy: true,
    };
  }
  const event = [...operations.values()]
    .sort((a, b) => a.time.localeCompare(b.time) || a.event_id.localeCompare(b.event_id))
    .at(-1);
  if (!event) return null;
  if (event.type === "session.compaction_started")
    return { phase: "running", message: translate("compaction.running"), busy: true };
  if (event.type === "session.compaction_failed") {
    return {
      phase: "failed",
      message:
        (event.payload.error_message as string | null) ||
        (event.payload.aborted
          ? translate("compaction.cancelled")
          : translate("compaction.failed")),
      busy: false,
    };
  }
  const before = event.payload.tokens_before;
  const after = event.payload.tokens_after;
  const message =
    typeof before === "number" && typeof after === "number"
      ? translate("compaction.tokenChange", { before, after })
      : translate("compaction.completed");
  return { phase: "completed", message, busy: false };
}

// These clients report durable completion facts but cannot read native checkpoints.
export function compactionEventItems(
  items: TurnTimelineItem[],
  events: EventView[],
): TurnTimelineItem[] {
  const completed = new Map<string, EventView>();
  for (const event of events) {
    if (event.type === "session.compaction_completed")
      completed.set(event.payload.operation_id as string, event);
  }
  const result = [...items];
  for (const event of [...completed.values()].sort((a, b) => a.time.localeCompare(b.time))) {
    const checkpoint: TurnTimelineItem = {
      item_id: `compaction-event:${event.event_id}`,
      turn_id: null,
      kind: "context_compaction",
      role: "system",
      raw_kind: event.type,
      title: null,
      status: "completed",
      occurred_at: event.time,
      content_preview: "",
    };
    const next = result.findIndex((item) => item.occurred_at && item.occurred_at > event.time);
    result.splice(next < 0 ? result.length : next, 0, checkpoint);
  }
  return result;
}
