import { onDestroy, tick } from "svelte";
import { fromStore } from "svelte/store";
import { translate } from "$lib/i18n";
import { highlightMarkdownCode } from "$lib/components/ai-elements/message/markdownHighlighter";
import { readTextPreview, textFileLanguage, TextPreviewError } from "$lib/downloads/textPreview";
import { downloads, downloadKey } from "$lib/downloads/manager";
import { copyText } from "$lib/copyText";
import { ApiError } from "../../api/errors";
import {
  provideFilePreviewNavigator,
  type FilePreviewTarget,
} from "../../components/chat/filePreviewContext";
import { toast } from "svelte-sonner";
/** One file in the panel's navigation history, with the reading position to restore on return. */
type HistoryEntry = { path: string; anchor: string | null; view: string; scrollTop: number };
type PreviewFailure =
  | { kind: "missing" | "tooLarge" | "notText" }
  | { kind: "failed"; message: string };

export function createFilePreview(
  target: FilePreviewTarget,
  openFile?: (next: FilePreviewTarget) => void,
) {
  const tasks = fromStore(downloads);
  // The panel is recreated for each target opened from outside, so history starts from the initial target.
  let history = $state<HistoryEntry[]>([
    { path: target.path, anchor: target.anchor ?? null, view: "preview", scrollTop: 0 },
  ]);
  const current = $derived(history[history.length - 1]);
  const path = $derived(current.path);
  const name = $derived(path.slice(path.lastIndexOf("/") + 1));
  const language = $derived(textFileLanguage(path));
  const htmlPreview = $derived(language === "html" && name.includes("."));
  const delimiter = $derived(/\.csv$/i.test(name) ? "," : /\.tsv$/i.test(name) ? "\t" : null);
  const treeFormat = $derived(
    name.includes(".") && (language === "json" || language === "yaml") ? language : null,
  );
  const hasPreview = $derived(
    language === "markdown" || htmlPreview || delimiter !== null || treeFormat !== null,
  );
  const key = $derived(downloadKey(target.device.id, target.sessionId, { path }));
  const task = $derived(tasks.current.find((task) => task.key === key));
  const receiving = $derived(task?.status === "receiving");
  let text = $state<string | null>(null);
  let failure = $state<PreviewFailure | null>(null);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  let view = $state("preview");
  let panel: HTMLElement;
  const highlighted = $derived(
    text === null || (hasPreview && view === "preview")
      ? ""
      : highlightMarkdownCode(text, language),
  );

  provideFilePreviewNavigator({
    open(next) {
      if (next.device !== target.device || next.sessionId !== target.sessionId) return;
      if (next.path === path) {
        if (next.anchor) void scrollToAnchor(next.anchor);
        return;
      }
      if (openFile) {
        openFile(next);
        return;
      }
      history[history.length - 1] = { ...current, view, scrollTop: scroller()?.scrollTop ?? 0 };
      history.push({ path: next.path, anchor: next.anchor ?? null, view: "preview", scrollTop: 0 });
    },
  });

  $effect(() => {
    const { device, sessionId } = target;
    const entry = current;
    const controller = new AbortController();
    text = null;
    failure = null;
    copied = false;
    view = entry.anchor ? "preview" : entry.view;
    void device.fileApi
      .pathFileContent(sessionId, entry.path, controller.signal)
      .then(readTextPreview)
      .then(async (content) => {
        if (controller.signal.aborted) return;
        text = content;
        await tick();
        if (controller.signal.aborted) return;
        if (entry.anchor) await scrollToAnchor(entry.anchor);
        else if (entry.scrollTop) scroller()?.scrollTo({ top: entry.scrollTop });
      })
      .catch((cause) => {
        if (!controller.signal.aborted) failure = previewFailure(cause);
      });
    return () => controller.abort();
  });

  onDestroy(() => clearTimeout(copyTimer));

  function scroller() {
    return panel.querySelector<HTMLElement>("[data-file-preview-content]:not([hidden])");
  }

  async function scrollToAnchor(anchor: string) {
    if (language !== "markdown") return;
    view = "preview";
    await tick();
    const heading = [...(scroller()?.querySelectorAll("[id]") ?? [])].find(
      (element) => element.id === anchor,
    );
    heading?.scrollIntoView({ block: "start" });
  }

  function back() {
    history.pop();
  }

  function refresh() {
    const content = scroller();
    history[history.length - 1] = content
      ? { path, anchor: null, view, scrollTop: content.scrollTop }
      : { ...current };
  }

  async function copy() {
    if (text === null) return;
    copied = await copyText(text);
    clearTimeout(copyTimer);
    if (copied)
      copyTimer = setTimeout(() => {
        copied = false;
      }, 1600);
    else toast.error(translate("files.copyFailed"));
  }

  function download() {
    downloads.start(target.device.id, target.sessionId, { path }, name);
  }

  function previewFailure(cause: unknown): PreviewFailure {
    if (cause instanceof ApiError && cause.status === 404) return { kind: "missing" };
    if (cause instanceof TextPreviewError) return { kind: cause.reason };
    return { kind: "failed", message: cause instanceof Error ? cause.message : String(cause) };
  }
  return {
    refresh,
    copy,
    download,
    back,
    get canGoBack() {
      return history.length > 1;
    },
    set panel(node: HTMLElement) {
      panel = node;
    },
    get panel() {
      return panel;
    },
    get view() {
      return view;
    },
    set view(next: string) {
      view = next;
    },
    get text() {
      return text;
    },
    get failure() {
      return failure;
    },
    get name() {
      return name;
    },
    get highlighted() {
      return highlighted;
    },
    get receiving() {
      return receiving;
    },
    get task() {
      return task;
    },
    get path() {
      return path;
    },
    get treeFormat() {
      return treeFormat;
    },
    get htmlPreview() {
      return htmlPreview;
    },
    get delimiter() {
      return delimiter;
    },
    get hasPreview() {
      return hasPreview;
    },
    get copied() {
      return copied;
    },
  };
}
