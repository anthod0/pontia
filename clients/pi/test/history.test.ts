import { expect, test } from "vitest";
import { readHistory, type HistoryContext } from "../src/history.js";
import { RpcError } from "../src/control-socket.js";
function fixture(count = 5) {
  const entries = Array.from({ length: count }, (_, i) => ({
    id: `e${i}`,
    parentId: i ? `e${i - 1}` : null,
    type: "custom",
    timestamp: "2026-10-11T00:00:00Z",
    data: { i },
  }));
  const context: HistoryContext = {
    generation: 1,
    sessionManager: {
      getSessionId: () => "native",
      getEntries: () => entries,
      getLeafId: () => entries.at(-1)?.id ?? null,
    },
  };
  return { context, entries };
}
test("pages a fixed append snapshot while later entries are appended", () => {
  const { context, entries } = fixture();
  const first = readHistory(context, { page_size: 2 }) as any;
  entries.push({ ...entries[4], id: "later", parentId: "e4" });
  const second = readHistory(context, {
    page_size: 2,
    snapshot: first.snapshot,
    continuation: first.continuation,
  }) as any;
  const third = readHistory(context, {
    page_size: 2,
    snapshot: second.snapshot,
    continuation: second.continuation,
  }) as any;
  expect([...first.entries, ...second.entries, ...third.entries].map((e) => e.id)).toEqual([
    "e0",
    "e1",
    "e2",
    "e3",
    "e4",
  ]);
  expect(third.continuation).toBeNull();
  expect(third.leaf_id).toBe("e4");
  expect([first.entry_count, second.entry_count, third.entry_count]).toEqual([5, 5, 5]);
  expect(entries.at(-1)?.id).toBe("later");
});
test("splits history larger than a frame into byte-bounded pages without truncation", () => {
  const { context, entries } = fixture(8);
  for (const entry of entries) (entry.data as any).text = "中".repeat(120_000);
  const ids: string[] = [];
  let params: any = { page_size: 128 };
  do {
    const page = readHistory(context, params) as any;
    expect(Buffer.byteLength(JSON.stringify(page))).toBeLessThan(1024 * 1024);
    ids.push(...page.entries.map((e: any) => e.id));
    params = page.continuation
      ? { snapshot: page.snapshot, continuation: page.continuation }
      : null;
  } while (params);
  expect(ids).toEqual(["e0", "e1", "e2", "e3", "e4", "e5", "e6", "e7"]);
});
test.each([0, -1, 129, 1.5, "2", null])("rejects invalid page size %s", (page_size) => {
  expect(() => readHistory(fixture().context, { page_size })).toThrow(RpcError);
});
test.each([1, 128])(
  "pages large images without changing native messages (page size %s)",
  (page_size) => {
    const { context } = fixture();
    const entries = ["user", "toolResult", "custom"].map((role, index) => ({
      id: `image-${index}`,
      parentId: index ? `image-${index - 1}` : null,
      type: "message",
      timestamp: "2026-11-05T00:00:00Z",
      message: {
        role,
        ...(role === "toolResult"
          ? { toolCallId: "call-1", toolName: "read", isError: false }
          : {}),
        content: [
          { type: "text", text: "中".repeat(120_000), data: "keep non-image data" },
          { type: "image", mimeType: "image/png", data: "a".repeat(2_546_580) },
          { type: "image", mimeType: "image/jpeg", data: "small image" },
        ],
      },
    }));
    context.sessionManager.getEntries = () => entries;
    context.sessionManager.getLeafId = () => "image-2";
    const original = JSON.stringify(entries);
    const received: any[] = [];
    let params: any = { page_size };
    do {
      const page = readHistory(context, params) as any;
      expect(Buffer.byteLength(JSON.stringify(page))).toBeLessThan(1024 * 1024);
      expect(page.entry_count).toBe(3);
      expect(page.upper_entry_id).toBe("image-2");
      expect(page.leaf_id).toBe("image-2");
      received.push(...page.entries);
      params = page.continuation
        ? { page_size, snapshot: page.snapshot, continuation: page.continuation }
        : null;
    } while (params);
    expect(received).toEqual(
      entries.map((entry) => ({
        ...entry,
        message: {
          ...entry.message,
          content: [
            entry.message.content[0],
            { type: "image", mimeType: "image/png" },
            { type: "image", mimeType: "image/jpeg" },
          ],
        },
      })),
    );
    expect(JSON.stringify(entries)).toBe(original);
  },
);

test("preserves image-shaped custom data outside message content", () => {
  const { context, entries } = fixture(1);
  Object.assign(entries[0].data, { type: "image", data: "custom payload" });
  expect((readHistory(context, {}) as any).entries).toEqual(entries);
});

test.each(["duplicate", "parent", "leaf", "timestamp", "oversized"])(
  "rejects %s snapshot evidence",
  (kind) => {
    const { context, entries } = fixture();
    if (kind === "duplicate") entries[4].id = "e0";
    if (kind === "parent") entries[1].parentId = "e4";
    if (kind === "leaf") context.sessionManager.getLeafId = () => "missing";
    if (kind === "timestamp") entries[2].timestamp = "";
    if (kind === "oversized") (entries[0].data as any).text = "x".repeat(1024 * 1024);
    expect(() => readHistory(context, {})).toThrow(RpcError);
  },
);
test.each(["native", "context", "upper", "continuation", "omission"])(
  "rejects %s changes during pagination",
  (kind) => {
    const { context, entries } = fixture();
    const page = readHistory(context, { page_size: 1 }) as any;
    if (kind === "native") context.sessionManager.getSessionId = () => "other";
    if (kind === "context") context.generation++;
    if (kind === "upper") entries.pop();
    if (kind === "continuation") page.continuation = "invalid";
    if (kind === "omission") {
      entries[2].parentId = "e0";
      entries.splice(1, 1);
    }
    expect(() =>
      readHistory(context, { snapshot: page.snapshot, continuation: page.continuation }),
    ).toThrow(RpcError);
  },
);
test("returns an empty snapshot and preserves every native entry kind", () => {
  expect(readHistory(fixture(0).context, {})).toMatchObject({
    entries: [],
    upper_entry_id: null,
    leaf_id: null,
    continuation: null,
  });
  const { context, entries } = fixture(12);
  const types = [
    "message",
    "thinking_level_change",
    "model_change",
    "usage",
    "compaction",
    "branch_summary",
    "custom",
    "custom_message",
    "context_edit",
    "label",
    "session_info",
    "future_kind",
  ];
  entries.forEach((entry, i) => {
    entry.type = types[i];
  });
  expect((readHistory(context, {}) as any).entries).toEqual(entries);
});
