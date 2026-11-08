import type { SessionView, TurnTimelineGroup, TurnTimelineItem } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createCompactionHistory(device: DeviceRuntime) {
  async function treeTimelineWithCheckpoints(
    sessionId: string,
    groups: TurnTimelineGroup[],
    limit: number,
  ): Promise<{ items: TurnTimelineItem[]; error: string | null }> {
    const items = groups.flatMap((group) => group.items);
    const session = device.queryClient.getQueryData<SessionView>(
      device.sessionQueries.sessionKeys.detail(sessionId),
    );
    if (!session?.capabilities.compaction_history) return { items, error: null };

    // Tree history owns the branch; session history supplies checkpoints between Turns.
    const history: TurnTimelineItem[] = [];
    const firstTurnId = groups[0]?.turn_id;
    let cursor = groups.at(-1)?.turn_id ?? null;
    const visited = new Set<string | null>();
    try {
      do {
        if (visited.has(cursor)) throw new Error("Compaction history returned a repeated cursor");
        visited.add(cursor);
        const page = await device.timelineQueries.fetchTurnTimeline(
          sessionId,
          "backward",
          cursor,
          limit,
        );
        history.unshift(...page.items);
        cursor = page.next_turn_id;
      } while (cursor && firstTurnId && cursor >= firstTurnId);
    } catch (error) {
      // A missing range in session history must not hide readable branch history.
      return { items, error: error instanceof Error ? error.message : String(error) };
    }

    if (!groups.length)
      return {
        items: history.filter(
          (item) => item.kind === "context_compaction" && item.turn_id === null,
        ),
        error: null,
      };

    const groupIds = new Set(groups.map((group) => group.turn_id));
    const existingIds = new Set(items.map((item) => item.item_id));
    const before = new Map<string, TurnTimelineItem[]>();
    const after = new Map<string, TurnTimelineItem[]>();
    for (let index = 0; index < history.length; index++) {
      const item = history[index];
      if (
        item.kind !== "context_compaction" ||
        item.turn_id !== null ||
        existingIds.has(item.item_id)
      )
        continue;
      const next = history.slice(index + 1).find((entry) => entry.turn_id !== null);
      const previous = next
        ? undefined
        : history.slice(0, index).findLast((entry) => entry.turn_id !== null);
      const owner = next?.turn_id ?? previous?.turn_id;
      if (!owner || !groupIds.has(owner)) continue;
      const target = next ? before : after;
      target.set(owner, [...(target.get(owner) ?? []), item]);
      existingIds.add(item.item_id);
    }
    return {
      items: groups.flatMap((group) => [
        ...(before.get(group.turn_id) ?? []),
        ...group.items,
        ...(after.get(group.turn_id) ?? []),
      ]),
      error: null,
    };
  }
  return { treeTimelineWithCheckpoints };
}
