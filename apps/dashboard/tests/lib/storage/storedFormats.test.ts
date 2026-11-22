import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { DeviceRuntime } from "../../../src/device/runtime";

// Saved data from earlier releases must stay readable, so every key keeps its name and format.
beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.resetModules();
});

afterEach(() => {
  vi.restoreAllMocks();
});

test("theme and language save the choice as text and remove the key for system", async () => {
  const { themePreference } = await import("../../../src/lib/theme");
  const { languagePreference } = await import("../../../src/lib/i18n");
  themePreference.set("dark");
  languagePreference.set("zh-CN");
  expect(localStorage.getItem("pontia.theme")).toBe("dark");
  expect(localStorage.getItem("pontia.locale")).toBe("zh-CN");
  themePreference.set("system");
  languagePreference.set("system");
  expect(localStorage.getItem("pontia.theme")).toBeNull();
  expect(localStorage.getItem("pontia.locale")).toBeNull();
});

test("saved theme and language are read back", async () => {
  localStorage.setItem("pontia.theme", "light");
  localStorage.setItem("pontia.locale", "en");
  expect(get((await import("../../../src/lib/theme")).themePreference)).toBe("light");
  expect(get((await import("../../../src/lib/i18n")).languagePreference)).toBe("en");
});

test("the External API token is saved as text, and an empty token as an empty string", async () => {
  const { loadTokenFromStorage, token } = await import("../../../src/stores/auth");
  token.set("secret");
  expect(localStorage.getItem("pontia.externalApiToken")).toBe("secret");
  token.set("");
  expect(localStorage.getItem("pontia.externalApiToken")).toBe("");
  localStorage.setItem("pontia.externalApiToken", "saved");
  expect(loadTokenFromStorage()).toBe("saved");
});

test("the file preview width is saved as a number and removed on reset", async () => {
  localStorage.setItem("pontia.filePreviewWidth", "500");
  const { filePreviewWidth } = await import("../../../src/components/chat/filePreviewWidth");
  expect(get(filePreviewWidth)).toBe(500);
  filePreviewWidth.save(432);
  expect(localStorage.getItem("pontia.filePreviewWidth")).toBe("432");
  filePreviewWidth.save(null);
  expect(localStorage.getItem("pontia.filePreviewWidth")).toBeNull();
});

test("the last page is saved as JSON per device and the opened tab as a session flag", async () => {
  const { forgetLastPage, openDashboardTab, rememberLastPage } =
    await import("../../../src/lib/lastPage");
  rememberLastPage("office-mac", { kind: "bot", id: "bot-1" });
  expect(localStorage.getItem("pontia.lastPage:office-mac")).toBe('{"kind":"bot","id":"bot-1"}');
  forgetLastPage("office-mac", { kind: "bot", id: "bot-1" });
  expect(localStorage.getItem("pontia.lastPage:office-mac")).toBeNull();
  openDashboardTab();
  expect(sessionStorage.getItem("pontia.tabOpened")).toBe("1");
});

test("a Bot quote draft is saved as JSON per device and Bot and removed when cleared", async () => {
  const { createQuoteDraft } = await import("../../../src/lib/botQuoteDraft");
  const key = 'pontia:bot-quote:v1:["local","bot-1"]';
  const quote = { kind: "file", fileId: "file-1", path: "a.txt", name: "a.txt" } as const;
  const draft = createQuoteDraft("local", "bot-1");
  draft.current = quote;
  expect(localStorage.getItem(key)).toBe(JSON.stringify(quote));
  expect(createQuoteDraft("local", "bot-1").current).toEqual(quote);
  expect(createQuoteDraft("local", "bot-2").current).toBeNull();
  draft.current = null;
  expect(localStorage.getItem(key)).toBeNull();
});

test("recent Bot files are saved as a JSON list per device and Bot and removed when cleared", async () => {
  const { createRecentFiles } = await import("../../../src/lib/botRecentFiles");
  const key = 'pontia.botRecentFiles:["local","bot-1"]';
  const files = [{ sessionId: "main", path: "/work/a.md" }];
  const recent = createRecentFiles("local", "bot-1");
  recent.current = files;
  expect(localStorage.getItem(key)).toBe(JSON.stringify(files));
  recent.current = [];
  expect(localStorage.getItem(key)).toBeNull();
});

test("history forks are saved as a JSON list per device", async () => {
  const { createHistoryForks } = await import("../../../src/stores/historyForks");
  const api = { forkSession: vi.fn().mockRejectedValue(new Error("offline")) };
  const forks = createHistoryForks({ id: "local", api } as unknown as DeviceRuntime);
  const operation = await forks.start("source", "turn", "input");
  expect(JSON.parse(localStorage.getItem("pontia:history-forks:v1:local")!)).toEqual([operation]);
  forks.forget(operation.operationId);
  expect(localStorage.getItem("pontia:history-forks:v1:local")).toBe("[]");
});

test("unconfirmed Inbox submissions are saved as a JSON list per device", async () => {
  const { createInboxRecovery } = await import("../../../src/stores/inboxRecovery");
  const recovery = createInboxRecovery({ id: "local" } as DeviceRuntime);
  const submission = { messageId: "m1", sessionId: "s1", input: { input: "hello" } };
  recovery.rememberSubmission(submission);
  expect(localStorage.getItem("pontia:inbox-submissions:v2:local")).toBe(
    JSON.stringify([submission]),
  );
  recovery.forgetSubmission("m1");
  expect(localStorage.getItem("pontia:inbox-submissions:v2:local")).toBe("[]");
});

test("a removed upload is marked with 1 when IndexedDB cannot delete it", async () => {
  const { removeUploadRecord } = await import("../../../src/lib/uploads/records");
  const record = {
    scope: "s",
    draft: "d",
    key: "k",
    uploadId: null,
    name: "x",
    size: 0,
    sha256: "",
  };
  vi.spyOn(IDBObjectStore.prototype, "delete").mockImplementation(() => {
    throw new Error("unavailable");
  });
  await removeUploadRecord(record);
  expect(localStorage.getItem('pontia-upload-removed:["s","d","k"]')).toBe("1");
});

test("New Chat remembers the client and workspace per device as text", async () => {
  // Formats written by NewChatPage, which tests/pages/chat/newChatPage.test.ts drives.
  const { readStored, scopedKey } = await import("../../../src/lib/storage");
  expect(scopedKey("pontia.chat.lastWorkspaceId", "local")).toBe(
    "pontia.chat.lastWorkspaceId:local",
  );
  localStorage.setItem("pontia.chat.lastCreationOptionId:local", "codex");
  expect(readStored(scopedKey("pontia.chat.lastCreationOptionId", "local"))).toBe("codex");
});

test("device handles are saved as text", async () => {
  // The keys DashboardModeLayout uses; tests/public/multiDeviceDashboard.test.ts covers the behavior.
  const { PersistedState, textSerializer } = await import("../../../src/lib/storage");
  localStorage.setItem("pontia.previousDeviceHandle", "office-mac");
  const last = new PersistedState("pontia.lastDeviceHandle", null, textSerializer);
  expect(new PersistedState("pontia.previousDeviceHandle", null, textSerializer).current).toBe(
    "office-mac",
  );
  last.current = "travel-laptop";
  expect(localStorage.getItem("pontia.lastDeviceHandle")).toBe("travel-laptop");
});

test.each([
  ["Inbox submissions", "../../../src/stores/inboxRecovery"],
  ["history forks", "../../../src/stores/historyForks"],
])("%s fail instead of reading as empty when storage is unavailable", async (_name, path) => {
  const module = await import(/* @vite-ignore */ path);
  const create = module.createInboxRecovery ?? module.createHistoryForks;
  vi.spyOn(window, "localStorage", "get").mockImplementation(() => {
    throw new DOMException("denied", "SecurityError");
  });
  expect(() => create({ id: "local" } as DeviceRuntime)).toThrow(DOMException);
});

test("an Inbox submission that cannot be saved is not remembered", async () => {
  const { createInboxRecovery } = await import("../../../src/stores/inboxRecovery");
  const recovery = createInboxRecovery({ id: "local" } as DeviceRuntime);
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("full", "QuotaExceededError");
  });
  expect(() =>
    recovery.rememberSubmission({ messageId: "m", sessionId: "s", input: { input: "x" } }),
  ).toThrow(DOMException);
});
