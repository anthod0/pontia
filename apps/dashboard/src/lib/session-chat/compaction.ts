import { translate as defaultTranslate, type Translator } from "$lib/i18n";
import { formatTokenCount } from "$lib/contextUsage";
import type { EventView, SessionView, TimelineItem, TurnTimelineItem } from "../../api/types";

export interface CompactionTokens {
  before: number | null;
  after: number | null;
}

/** A checkpoint synthesized from a completion fact carries the reported token counts. */
export type CompactionCheckpointItem = TimelineItem & { compaction_tokens?: CompactionTokens };
type CompactionTimelineItem = TurnTimelineItem & CompactionCheckpointItem;

function tokenCount(value: unknown): number | null {
  return typeof value === "number" ? value : null;
}

export function compactionTokens(event: EventView): CompactionTokens {
  return {
    before: tokenCount(event.payload.tokens_before),
    after: tokenCount(event.payload.tokens_after),
  };
}

export function compactionCompletedLabel(
  tokens: CompactionTokens | undefined,
  translate: Translator = defaultTranslate,
): string {
  const before = tokens?.before ?? null;
  const after = tokens?.after ?? null;
  if (before !== null && after !== null)
    return translate("compaction.tokenChange", {
      before: formatTokenCount(before),
      after: formatTokenCount(after),
    });
  if (before !== null)
    return translate("compaction.tokensBefore", { before: formatTokenCount(before) });
  if (after !== null)
    return translate("compaction.tokensAfter", { after: formatTokenCount(after) });
  return translate("compaction.completed");
}

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
  return {
    phase: "completed",
    message: compactionCompletedLabel(compactionTokens(event), translate),
    busy: false,
  };
}

// These clients report durable completion facts but cannot read native checkpoints.
export function compactionEventItems(
  items: TurnTimelineItem[],
  events: EventView[],
): CompactionTimelineItem[] {
  const completed = new Map<string, EventView>();
  for (const event of events) {
    if (event.type === "session.compaction_completed")
      completed.set(event.payload.operation_id as string, event);
  }
  const result: CompactionTimelineItem[] = [...items];
  for (const event of [...completed.values()].sort((a, b) => a.time.localeCompare(b.time))) {
    const checkpoint: CompactionTimelineItem = {
      item_id: `compaction-event:${event.event_id}`,
      turn_id: null,
      kind: "context_compaction",
      role: "system",
      raw_kind: event.type,
      title: null,
      status: "completed",
      occurred_at: event.time,
      content_preview: "",
      compaction_tokens: compactionTokens(event),
    };
    const next = result.findIndex((item) => item.occurred_at && item.occurred_at > event.time);
    result.splice(next < 0 ? result.length : next, 0, checkpoint);
  }
  return result;
}
