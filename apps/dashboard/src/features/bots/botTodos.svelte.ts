import { toast } from "svelte-sonner";
import { translate } from "$lib/i18n";
import type { BotMessage } from "../../api/types";
import { createQueries } from "@tanstack/svelte-query";
import type { DeviceRuntime } from "../../device/runtime";
import type { createBotDirectory } from "./botDirectory.svelte";

export function createBotTodos(
  device: DeviceRuntime,
  directory: ReturnType<typeof createBotDirectory>,
) {
  const bots = $derived(directory.bots ?? []);
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
export async function setBotTodo(
  device: DeviceRuntime,
  botId: string,
  id: string,
  open: boolean,
  updated?: (message: BotMessage) => void,
  failed?: () => void,
) {
  try {
    const message = await device.api.setBotTodo(botId, id, open);
    updated?.(message);
    void device.queryClient.invalidateQueries({ queryKey: ["bot-message-context", botId] });
    if (!open && message.todo?.state === "done" && message.todo.by === "user") {
      toast.success(translate("bots.todoCancelled"), {
        id: message.id,
        action: {
          label: translate("common.undo"),
          onClick: () => void setBotTodo(device, botId, id, true, updated, failed),
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
