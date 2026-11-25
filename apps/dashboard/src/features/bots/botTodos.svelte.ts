import { toast } from "svelte-sonner";
import { translate } from "$lib/i18n";
import type { BotMessage } from "../../api/types";
import { createQueries } from "@tanstack/svelte-query";
import type { DeviceRuntime } from "../../device/runtime";
import type { createBotDirectory } from "./botDirectory.svelte";

export function createBotTodos(
  device: DeviceRuntime,
  directory: ReturnType<typeof createBotDirectory>,
  includeRecentlyResolved = false,
) {
  const bots = $derived(
    (directory.bots ?? []).filter(
      (_, i) => includeRecentlyResolved || (directory.summaries[i]?.open_todo_count ?? 0) > 0,
    ),
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
export async function changeBotTodo(
  device: DeviceRuntime,
  botId: string,
  id: string,
  action: "resolve" | "reopen",
  updated?: (message: BotMessage) => void,
  failed?: () => void,
) {
  try {
    const message = await device.api.changeBotTodo(botId, id, action);
    updated?.(message);
    if (action === "resolve" && message.todo?.state === "done" && message.todo.by === "user") {
      toast.success(translate("bots.ignored"), {
        id: message.id,
        action: {
          label: translate("common.undo"),
          onClick: () => void changeBotTodo(device, botId, id, "reopen", updated, failed),
        },
      });
    }
  } catch (failure) {
    toast.error(failure instanceof Error ? failure.message : String(failure));
    failed?.();
  } finally {
    void device.queryClient.invalidateQueries({ queryKey: ["bot-todos", botId] });
    void device.queryClient.invalidateQueries({ queryKey: ["bot-summary", botId] });
  }
}
