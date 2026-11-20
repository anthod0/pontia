import type {
  Bot,
  BotEntry,
  BotMessage,
  BotMessagePage,
  BotNotice,
  BotWorkStatus,
} from "../api/types";
import { ApiError } from "../api/errors";
import type { DeviceRuntime } from "../device/runtime";

export function compareBotEntries(a: BotEntry, b: BotEntry): number {
  return (
    Date.parse(a.created_at) - Date.parse(b.created_at) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  );
}

function equalEntry(a: BotEntry, b: BotEntry): boolean {
  return (
    a.kind === b.kind &&
    a.created_at === b.created_at &&
    (!("content" in a) || ("content" in b && a.content === b.content && a.status === b.status))
  );
}

export class BotTimeline {
  entries = $state.raw<BotEntry[]>([]);
  bot = $state.raw<Bot | null>(null);
  fileLinkSession = $state.raw<BotMessagePage["file_link_session"]>(null);
  status = $state.raw<BotWorkStatus>({ kind: "idle" });
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
  private rebaseHistory = false;
  private pending: "changed" | "reset" | null = null;
  private running = false;
  private olderRequested = false;
  private verifyRequested = false;
  private live = new Map<string, BotMessage>();

  constructor(
    private device: DeviceRuntime,
    private id: string,
  ) {}

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

  private async page(locator: { before?: string; around?: string } = {}) {
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

  private async sync(reset: boolean) {
    const latest = await this.page();
    const rotated = this.bot !== null && latest.notice_session_id !== this.noticeSession;
    if (rotated) {
      this.entries = this.entries.filter((row) => "content" in row || row.kind === "shared_file");
      this.known.clear();
      for (const row of this.entries) this.known.set(row.id, row);
      this.rebaseHistory = this.before !== null;
      this.before = null;
    }
    if (reset || rotated) this.stale = new Set(this.known.keys());
    this.bot = latest.bot;
    this.fileLinkSession = latest.file_link_session;
    this.status = latest.status;
    this.device.queryClient.setQueryData(["bot-status", this.id], latest.status);
    this.generationNotice = latest.generation_notice;
    this.device.queryClient.setQueryData<Bot[]>(["bots"], (bots) =>
      bots?.map((bot) => (bot.bot_id === this.id ? latest.bot : bot)),
    );
    const oldest = this.entries[0];
    this.noticeSession = latest.notice_session_id;
    if (reset || rotated) {
      for (const row of latest.entries) this.stale.delete(row.id);
      await this.verifyVisible();
    }
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
        (this.pending || this.olderRequested || this.verifyRequested) &&
        !this.controller.signal.aborted
      ) {
        this.live.clear();
        const older = this.olderRequested && this.pending !== "reset";
        const refresh = older ? null : this.pending;
        if (refresh) this.pending = null;
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
          } else {
            this.verifyRequested = false;
            await this.verifyVisible();
          }
        } catch (error) {
          if (this.controller.signal.aborted) return;
          if (error instanceof ApiError && error.code === "bot_timeline_reset") {
            this.pending = "reset";
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
    }
  }
}
