import { createQueries } from "@tanstack/svelte-query";
import type { DeviceRuntime } from "../../device/runtime";
import type { createBotDirectory } from "./botDirectory.svelte";

export function createBotTodos(
  device: DeviceRuntime,
  directory: ReturnType<typeof createBotDirectory>,
) {
  const bots = $derived(
    (directory.bots ?? []).filter((_, i) => (directory.summaries[i]?.open_todo_count ?? 0) > 0),
  );
  const queries = createQueries(
    () => ({
      queries: bots.map((bot) => ({
        queryKey: ["bot-todos", bot.bot_id],
        queryFn: ({ signal }: { signal: AbortSignal }) =>
          device.api.getBotTodos(bot.bot_id, signal),
      })),
    }),
    () => device.queryClient,
  );
  const groups = $derived(bots.map((bot, i) => ({ bot, result: queries[i] })));
  return {
    get groups() {
      return groups;
    },
  };
}
