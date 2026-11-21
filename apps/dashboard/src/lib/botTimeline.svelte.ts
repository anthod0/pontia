import type {
  Bot,
  BotEntry,
  BotMessage,
  BotMessagePage,
  BotNotice,
  BotSharedFiles,
  BotWorkStatus,
  SharedFile,
} from "../api/types";
import { ApiError } from "../api/errors";
import type { DeviceRuntime } from "../device/runtime";
import {
  readCachedBotTimeline,
  removeCachedBotTimeline,
  writeCachedBotTimeline,
} from "./botTimelineCache";

export function compareBotEntries(a: BotEntry, b: BotEntry): number {
  return (
    Date.parse(a.created_at) - Date.parse(b.created_at) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  );
}

function equalEntry(a: BotEntry, b: BotEntry): boolean {
  return (
    a.kind === b.kind &&
    a.created_at === b.created_at &&
    (!("content" in a) ||
      ("content" in b &&
        a.content === b.content &&
        a.status === b.status &&
        a.todo?.state === b.todo?.state))
  );
}

export class BotTimeline {
  entries = $state.raw<BotEntry[]>([]);
  bot = $state.raw<Bot | null>(null);
  fileLinkSession = $state.raw<BotMessagePage["file_link_session"]>(null);
  status = $state.raw<BotWorkStatus>({ kind: "idle" });
  openTodoCount = $state(0);
  generationNotice = $state.raw<BotNotice | null>(null);
  before = $state<string | null>(null);
  loadingOlder = $state(false);
  historyError = $state("");
  refreshError = $state("");
  private readonly controller = new AbortController();
  private readonly known = new Map<string, BotEntry>();
  private visible: string[] = [];
  private stale = new Set<string>();
  private noticeSession: string | null = null;
  // Entries restored from the device cache, shown until the first latest page replaces them.
  private cached = false;
  private deleted = false;
  private rebaseHistory = false;
  private pending: "changed" | "reset" | null = null;
  private running = false;
  private olderRequested = false;
  private verifyRequested = false;
  private live = new Map<string, BotMessage>();
  private revealRequest: {
    id: string;
    resolve: (found: boolean) => void;
    reject: (error: unknown) => void;
  } | null = null;

  constructor(
    private device: DeviceRuntime,
    private id: string,
  ) {
    void this.restore();
  }

  destroy() {
    this.controller.abort();
  }

  refresh(reset = false) {
    if (reset || this.pending !== "reset") this.pending = reset ? "reset" : "changed";
    void this.drain();
  }

  loadOlder() {
    if (!this.before || this.loadingOlder) return;
    this.historyError = "";
    this.loadingOlder = this.olderRequested = true;
    void this.drain();
  }

  /** The loaded entry with this ID, or the file share containing this file ID. */
  entryId(id: string): string | null {
    return this.known.has(id) ? id : (this.sharedFile(id)?.share.id ?? null);
  }

  /** A loaded shared file and the share it belongs to. */
  sharedFile(fileId: string): { share: BotSharedFiles; file: SharedFile } | null {
    for (const row of this.known.values()) {
      if (row.kind !== "shared_files") continue;
      const file = row.files.find((file) => file.file_id === fileId);
      if (file) return { share: row, file };
    }
    return null;
  }

  /** Loads history back to an entry or shared file. Resolves false when this Bot has no such entry. */
  reveal(id: string): Promise<boolean> {
    if (this.entryId(id)) return Promise.resolve(true);
    return new Promise((resolve, reject) => {
      this.revealRequest?.resolve(false);
      this.revealRequest = { id, resolve, reject };
      void this.drain();
    });
  }

  setVisible(ids: string[]) {
    this.visible = ids;
    if (ids.some((id) => this.stale.has(id)) && !this.refreshError) {
      this.verifyRequested = true;
      void this.drain();
    }
  }

  message(message: BotMessage) {
    if (this.running) this.live.set(message.id, message);
    if (this.known.has(message.id)) this.merge([message]);
    this.refresh();
  }

  private async restore() {
    const snapshot = await readCachedBotTimeline(this.device.id, this.id);
    if (!snapshot || this.bot || this.controller.signal.aborted) return;
    this.cached = true;
    this.bot = snapshot.bot;
    this.openTodoCount = snapshot.openTodoCount ?? 0;
    this.fileLinkSession = snapshot.fileLinkSession;
    for (const row of snapshot.entries) this.known.set(row.id, row);
    this.entries = snapshot.entries;
  }

  private persist() {
    if (this.cached || this.deleted || !this.bot || this.controller.signal.aborted) return;
    void writeCachedBotTimeline({
      deviceId: this.device.id,
      botId: this.id,
      bot: this.bot,
      fileLinkSession: this.fileLinkSession,
      openTodoCount: this.openTodoCount,
      entries: this.entries,
    });
  }

  private async page(locator: { before?: string; around?: string; limit?: number } = {}) {
    const signal = this.controller.signal;
    const page = await this.device.api.getBotMessages(this.id, locator, signal);
    signal.throwIfAborted();
    return page;
  }

  private merge(rows: BotEntry[]) {
    const added: BotEntry[] = [];
    let changed = false;
    for (const row of rows) {
      const current = this.live.get(row.id) ?? row;
      const previous = this.known.get(row.id);
      if (!previous || !equalEntry(previous, current)) {
        if (!previous) added.push(current);
        this.known.set(row.id, current);
        changed = true;
      }
      this.stale.delete(row.id);
    }
    if (!changed) return;
    const existing = this.entries.map((row) => this.known.get(row.id)!);
    if (added.length === 0) {
      this.entries = existing;
      return;
    }
    added.sort(compareBotEntries);
    if (!existing.length || compareBotEntries(existing.at(-1)!, added[0]) < 0) {
      this.entries = [...existing, ...added];
    } else if (compareBotEntries(added.at(-1)!, existing[0]) < 0) {
      this.entries = [...added, ...existing];
    } else {
      this.entries = [...existing, ...added].sort(compareBotEntries);
    }
  }

  // Older pages are merged in order so the loaded history stays contiguous.
  private async loadThrough(id: string): Promise<boolean> {
    try {
      await this.page({ around: id, limit: 1 });
    } catch (error) {
      if (error instanceof ApiError && error.status === 404) return false;
      throw error;
    }
    while (!this.entryId(id) && this.before) {
      const page = await this.page({ before: this.before, limit: 100 });
      this.merge(page.entries);
      this.before = page.next_before;
    }
    return this.entryId(id) !== null;
  }

  private async verifyVisible() {
    // The feed supplies visible rows nearest its centre first. An expired notice
    // can disappear on rotation; the adjacent durable message is still usable.
    const anchor = this.visible.find((id) => this.stale.has(id));
    if (!anchor) return;
    try {
      const page = await this.page({ around: anchor });
      if (page.notice_session_id !== this.noticeSession) {
        this.pending = "reset";
        return;
      }
      this.merge(page.entries);
    } catch (error) {
      if (!(error instanceof ApiError && error.status === 404)) throw error;
      this.stale.delete(anchor);
      this.known.delete(anchor);
      this.entries = this.entries.filter((row) => row.id !== anchor);
      await this.verifyVisible();
    }
  }

  // A missing latest page means the Bot was deleted, possibly from another browser.
  private async latestPage() {
    try {
      return await this.page();
    } catch (error) {
      if (error instanceof ApiError && error.status === 404) {
        this.deleted = true;
        void removeCachedBotTimeline(this.device.id, this.id);
        if (this.cached) {
          this.cached = false;
          this.bot = null;
          this.known.clear();
          this.entries = [];
        }
      }
      throw error;
    }
  }

  private async sync(reset: boolean) {
    const latest = await this.latestPage();
    const rotated =
      !this.cached && this.bot !== null && latest.notice_session_id !== this.noticeSession;
    if (this.cached) {
      // The latest page replaces the cached one, dropping rows changed or removed since.
      this.cached = false;
      this.known.clear();
      this.entries = [];
      this.merge(latest.entries);
      this.before = latest.next_before;
    }
    if (rotated) {
      this.entries = this.entries.filter((row) => "content" in row || row.kind === "shared_files");
      this.known.clear();
      for (const row of this.entries) this.known.set(row.id, row);
      this.rebaseHistory = this.before !== null;
      this.before = null;
    }
    if (reset || rotated) this.stale = new Set(this.known.keys());
    this.bot = latest.bot;
    this.openTodoCount = latest.open_todo_count;
    this.fileLinkSession = latest.file_link_session;
    this.status = latest.status;
    this.device.queryClient.setQueryData(["bot-status", this.id], latest.status);
    this.generationNotice = latest.generation_notice;
    this.device.queryClient.setQueryData<Bot[]>(["bots"], (bots) =>
      bots?.map((bot) => (bot.bot_id === this.id ? latest.bot : bot)),
    );
    const oldest = this.entries[0];
    this.noticeSession = latest.notice_session_id;
    // A new reply can close a todo outside the latest page. Recheck it when visible.
    for (const row of this.known.values()) {
      if ("content" in row && row.todo?.state === "open") this.stale.add(row.id);
    }
    for (const row of latest.entries) this.stale.delete(row.id);
    await this.verifyVisible();
    const pending: BotEntry[] = [];
    let page: BotMessagePage = latest;
    // The cursor follows server order. Stage new rows until the range touches
    // the previous tail so a failed catch-up never advances past missing rows.
    while (oldest && page.next_before && !page.entries.some((row) => this.known.has(row.id))) {
      pending.push(...page.entries);
      page = await this.page({ before: page.next_before });
    }
    this.merge([...pending, ...page.entries]);
    if (!oldest || !page.next_before) this.before = page.next_before;
    if (this.rebaseHistory && oldest && page.next_before) {
      const boundary = await this.page({ around: oldest.id });
      this.merge(boundary.entries);
      this.before = boundary.next_before;
    }
    this.rebaseHistory = false;
  }

  private async drain() {
    if (this.running || this.controller.signal.aborted) return;
    this.running = true;
    try {
      while (
        (this.pending || this.olderRequested || this.verifyRequested || this.revealRequest) &&
        !this.controller.signal.aborted
      ) {
        this.live.clear();
        const older = this.olderRequested && this.pending !== "reset";
        const refresh = older ? null : this.pending;
        const reveal = older || refresh ? null : this.revealRequest;
        if (refresh) this.pending = null;
        if (reveal) this.revealRequest = null;
        try {
          if (refresh) {
            await this.sync(refresh === "reset");
            this.refreshError = "";
          } else if (older) {
            this.olderRequested = false;
            if (this.before) {
              const page = await this.page({ before: this.before });
              this.merge(page.entries);
              this.before = page.next_before;
            }
          } else if (reveal) {
            reveal.resolve(await this.loadThrough(reveal.id));
          } else {
            this.verifyRequested = false;
            await this.verifyVisible();
          }
        } catch (error) {
          if (this.controller.signal.aborted) return;
          if (error instanceof ApiError && error.code === "bot_timeline_reset") {
            this.pending = "reset";
            if (reveal) this.revealRequest ??= reveal;
          } else if (reveal) {
            reveal.reject(error);
          } else {
            const message = error instanceof Error ? error.message : String(error);
            if (older) this.historyError = message;
            else this.refreshError = message;
          }
        } finally {
          if (older) this.loadingOlder = false;
        }
      }
    } finally {
      this.live.clear();
      this.running = false;
      this.persist();
    }
  }
}
