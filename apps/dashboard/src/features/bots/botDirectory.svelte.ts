import { createQueries, createQuery } from "@tanstack/svelte-query";
import type { BotSummary } from "../../api/types";
import type { DeviceRuntime } from "../../device/runtime";
import { openBotChatStream } from "../../services/botChatStream";

export function createBotDirectory(device: DeviceRuntime) {
  const bots = createQuery(
    () => ({ queryKey: ["bots"], queryFn: ({ signal }) => device.api.listBots(signal) }),
    () => device.queryClient,
  );
  const summaries = createQueries(
    () => ({
      queries: (bots.data ?? []).map((bot) => ({
        queryKey: ["bot-summary", bot.bot_id],
        queryFn: async ({ signal }: { signal: AbortSignal }): Promise<BotSummary> => {
          const { status, open_todo_count } = await device.api.getBotMessages(
            bot.bot_id,
            { limit: 1 },
            signal,
          );
          return { status, open_todo_count };
        },
      })),
    }),
    () => device.queryClient,
  );
  const botIds = $derived((bots.data ?? []).map((bot) => bot.bot_id).join(","));
  $effect(() => {
    const close = botIds
      .split(",")
      .filter(Boolean)
      .map((id) =>
        openBotChatStream(
          device,
          id,
          (update) => {
            if (update.type === "typing") return;
            void device.queryClient.invalidateQueries({ queryKey: ["bot-todos", id] });
            const query = { queryKey: ["bot-summary", id] };
            // Invalidation alone can reuse an initial request that predates this update.
            void device.queryClient
              .cancelQueries(query)
              .then(() => device.queryClient.invalidateQueries(query));
          },
          () => {},
        ),
      );
    return () => close.forEach((stop) => stop());
  });
  let reloading = $state<string | null>(null);
  async function reload(id: string) {
    reloading = id;
    try {
      await device.api.reloadBot(id);
    } finally {
      reloading = null;
    }
  }
  const data = $derived(summaries.map((query) => query.data));
  return {
    get error() {
      return bots.error ?? summaries.find((query) => query.error)?.error;
    },
    get pending() {
      return bots.isPending || summaries.some((query) => query.isPending);
    },
    get bots() {
      return bots.data;
    },
    get summaries() {
      return data;
    },
    get hasOpenTodos() {
      return data.some((summary) => (summary?.open_todo_count ?? 0) > 0);
    },
    get reloading() {
      return reloading;
    },
    reload,
  };
}
