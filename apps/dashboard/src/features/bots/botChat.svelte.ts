import { setBotTodo } from "./botTodos.svelte";
import { onDestroy, onMount, tick } from "svelte";
import { toast } from "svelte-sonner";
import { BotTimeline } from "$lib/botTimeline.svelte";
import { ApiError } from "../../api/errors";
import { useDeviceRuntime } from "../../device/context";
import { openBotChatStream } from "../../services/botChatStream";
import { quotedMessage, type BotQuote } from "$lib/botQuote";
import { isImagePath } from "$lib/downloads/localFilePath";
import { createQuoteDraft } from "$lib/botQuoteDraft";
import { translate } from "$lib/i18n";
export function createBotChat(botId: string) {
  const device = useDeviceRuntime();

  const timeline = new BotTimeline(device, botId);
  let draft = $state("");
  let typing = $state(false);
  let retrying = $state<string[]>([]);
  let feed = $state.raw<{ scrollToEntry(id: string): void; followSend(): void }>();
  let composer = $state.raw<{ focusInput(): void; submit(): boolean }>();
  const quoteDraft = createQuoteDraft(device.id, botId);
  const quote = $derived(quoteDraft.current);
  let highlighted = $state<string | null>(null);
  // The attached file to mark within the highlighted message.
  let highlightedPath = $state<string | null>(null);
  let jumping = $state<string | null>(null);
  let missingQuotes = $state<string[]>([]);
  let highlightTimer: ReturnType<typeof setTimeout> | undefined;
  const actionToastId = "bot-action-error";
  onDestroy(() => {
    clearTimeout(highlightTimer);
    toast.dismiss(actionToastId);
  });

  function showActionError(failure: unknown, retry: (event: MouseEvent) => void) {
    toast.error(failure instanceof Error ? failure.message : String(failure), {
      id: actionToastId,
      important: true,
      action: { label: translate("common.retry"), onClick: retry },
    });
  }

  function setQuote(next: BotQuote | null) {
    quoteDraft.current = next;
  }

  async function addQuote(next: BotQuote) {
    setQuote(next);
    await tick();
    composer?.focusInput();
  }

  async function jumpTo(id: string, path: string | null = null) {
    if (jumping) return;
    jumping = id;
    try {
      if (!(await timeline.reveal(id))) {
        missingQuotes = [...missingQuotes, id];
        toast.error(translate("bots.quoteMissing"));
        return;
      }
      await tick();
      feed?.scrollToEntry(id);
      highlighted = id;
      highlightedPath = path;
      clearTimeout(highlightTimer);
      highlightTimer = setTimeout(() => {
        highlighted = highlightedPath = null;
      }, 1500);
    } catch (failure) {
      toast.error(failure instanceof Error ? failure.message : String(failure));
    } finally {
      jumping = null;
    }
  }

  onMount(() => {
    const stream = openBotChatStream(
      device,
      botId,
      (update) => {
        typing = update.type === "typing";
        if (update.type !== "typing") {
          void device.queryClient.invalidateQueries({ queryKey: ["bot-message-context", botId] });
          void device.queryClient.invalidateQueries({ queryKey: ["bot-todos", botId] });
        }
        if (update.type === "message") timeline.message(update.message);
        else if (update.type === "changed" || update.type === "reset")
          timeline.refresh(update.type === "reset");
      },
      () => {
        typing = false;
      },
    );
    const resume = () => {
      if (!document.hidden) timeline.refresh(true);
    };
    document.addEventListener("visibilitychange", resume);
    window.addEventListener("online", resume);
    timeline.refresh();
    return () => {
      stream();
      timeline.destroy();
      document.removeEventListener("visibilitychange", resume);
      window.removeEventListener("online", resume);
    };
  });
  function showDelivery(message: Awaited<ReturnType<typeof device.api.sendBotMessage>>) {
    timeline.message(message);
    if (message.delivery === "queued") toast.info(translate("bots.queuedAfterTurn"));
  }
  // The composer and a todo's reply box both send through here, so a reply matches quoting first and sending from the composer.
  async function post(content: string, sentQuote: BotQuote | null) {
    toast.dismiss(actionToastId);
    feed?.followSend();
    const message = await device.api.sendBotMessage(
      botId,
      sentQuote ? quotedMessage(sentQuote, content) : content,
    );
    showDelivery(message);
    if (sentQuote && message.status === "failed") throw new Error(translate("bots.sendFailed"));
  }

  let reloading = $state(false);
  async function reload() {
    if (reloading) return;
    reloading = true;
    try {
      await device.api.reloadBot(botId);
      if (draft.trim() === "/reload") draft = "";
      toast.success(translate("bots.reloadReady"));
    } catch (error) {
      toast.error(
        error instanceof ApiError && error.code === "state_conflict"
          ? translate("bots.reloadBusy")
          : error instanceof Error
            ? error.message
            : String(error),
      );
    } finally {
      reloading = false;
    }
  }

  async function send(content: string): Promise<boolean> {
    const text = draft;
    const sentQuote = quote;
    draft = "";
    try {
      await post(content, sentQuote);
      if (quote === sentQuote) setQuote(null);
      return true;
    } catch (failure) {
      draft = draft ? text + "\n\n" + draft : text;
      showActionError(failure, (event) => {
        if (!composer?.submit()) event.preventDefault();
      });
      timeline.refresh();
      return false;
    }
  }

  async function replyTodo(todo: BotQuote, text: string, retry: () => boolean): Promise<boolean> {
    try {
      await post(text, todo);
      return true;
    } catch (failure) {
      showActionError(failure, (event) => {
        if (!retry()) event.preventDefault();
      });
      timeline.refresh();
      return false;
    }
  }

  function quoteImage(value: BotQuote) {
    const sessionId = timeline.fileLinkSession?.session_id;
    return value.kind === "file" && sessionId && isImagePath(value.path)
      ? { sessionId, path: value.path }
      : null;
  }

  function quoteSpeaker(value: BotQuote) {
    return value.kind === "message" && value.speaker === "User"
      ? translate("bots.you")
      : (timeline.bot?.name ?? "");
  }

  async function changeTodo(id: string, open: boolean) {
    await setBotTodo(
      device,
      botId,
      id,
      open,
      (message) => timeline.message(message),
      () => timeline.refresh(),
    );
  }

  async function retry(id: string) {
    retrying = [...retrying, id];
    toast.dismiss(actionToastId);
    try {
      showDelivery(await device.api.retryBotMessage(botId, id));
    } catch (failure) {
      showActionError(failure, () => {
        if (!retrying.includes(id)) void retry(id);
      });
      timeline.refresh();
    } finally {
      retrying = retrying.filter((messageId) => messageId !== id);
    }
  }
  return {
    botId,
    timeline,
    get draft() {
      return draft;
    },
    get typing() {
      return typing;
    },
    get retrying() {
      return retrying;
    },
    get quote() {
      return quote;
    },
    get highlighted() {
      return highlighted;
    },
    get highlightedPath() {
      return highlightedPath;
    },
    get jumping() {
      return jumping;
    },
    get missingQuotes() {
      return missingQuotes;
    },
    get reloading() {
      return reloading;
    },
    set draft(value: string) {
      draft = value;
    },
    get feed() {
      return feed;
    },
    set feed(value: typeof feed) {
      feed = value;
    },
    get composer() {
      return composer;
    },
    set composer(value: typeof composer) {
      composer = value;
    },
    jumpTo,
    addQuote,
    setQuote,
    retry,
    quoteImage,
    quoteSpeaker,
    changeTodo,
    post,
    replyTodo,
    send,
    reload,
  };
}
export type BotChatController = ReturnType<typeof createBotChat>;
