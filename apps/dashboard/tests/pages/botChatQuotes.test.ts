import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEventBase from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, BotEntry, BotMessage } from "../../src/api/types";
import { ApiError } from "../../src/api/errors";
import BotChatPage from "../../src/pages/BotChatPage.svelte";
import { saveUploadRecord } from "../../src/lib/uploads/records";

const toast = vi.hoisted(() => ({ error: vi.fn(), success: vi.fn(), warning: vi.fn() }));
vi.mock("svelte-sonner", () => ({ toast }));
vi.mock("../../src/services/botChatStream", () => ({ openBotChatStream: () => () => {} }));

const bot: Bot = {
  bot_id: "quotes",
  name: "Helper Bot",
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "test",
};
const id = (n: number) => `bmsg_0193a000-0000-7000-8000-${String(n).padStart(12, "0")}`;
const fileId = "file_0193a000-0000-7000-8000-000000000001";
const message = (n: number, kind: BotMessage["kind"], content: string): BotMessage => ({
  id: id(n),
  kind,
  content,
  status: "normal",
  created_at: new Date(n * 1000).toISOString(),
});
const reply = message(1, "bot_reply", "Renew the **certificate** first.");
const file: BotEntry = {
  kind: "shared_file",
  id: fileId,
  session_id: "s",
  created_at: new Date(2000).toISOString(),
  file: {
    file_id: fileId,
    turn_id: null,
    name: "checklist.md",
    path: "/work/checklist.md",
    size: 12,
    media_type: "text/markdown",
    created_at: new Date(2000).toISOString(),
  },
};

function serve(entries: BotEntry[], options: { older?: BotEntry[]; missing?: string[] } = {}) {
  return vi
    .spyOn(testDevice.api, "getBotMessages")
    .mockImplementation(async (_id, { before, around }) => {
      if (around !== undefined && options.missing?.includes(around))
        throw new ApiError("Bot timeline entry not found", "not_found", 404);
      const base = {
        bot,
        status: { kind: "idle" as const },
        file_link_session: null,
        notice_session_id: "s",
        generation_notice: null,
      };
      if (around !== undefined) return { ...base, entries: [], next_before: null };
      if (before) return { ...base, entries: options.older ?? [], next_before: null };
      return { ...base, entries, next_before: options.older ? "older" : null };
    });
}

function sent() {
  return vi
    .spyOn(testDevice.api, "sendBotMessage")
    .mockImplementation(async (_bot, content) => message(99, "user_input", content));
}

async function quoteFromMenu(row: HTMLElement) {
  await fireEvent.contextMenu(row, { clientX: 10, clientY: 10 });
  await fireEvent.click(await screen.findByRole("menuitem", { name: /Quote/ }));
}

// The closing menu briefly leaves pointer-events: none on the body in jsdom.
const userEvent = userEventBase.setup({ pointerEventsCheck: 0 });
const rows = () => screen.getAllByRole("article");

beforeEach(() => {
  localStorage.clear();
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("role") === "log" ? 2000 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(56);
});
afterEach(() => {
  vi.restoreAllMocks();
  toast.error.mockReset();
});

test("sends a quoted Bot reply as a user_quote element before the typed text", async () => {
  serve([reply]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  await quoteFromMenu(rows()[0]);
  await userEvent.type(screen.getByRole("textbox"), "Who owns this?");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      bot.bot_id,
      `<user_quote message_id="${id(1)}" speaker="Bot">Renew the **certificate** first.</user_quote>\nWho owns this?`,
    ),
  );
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: /^Remove quote/ })).not.toBeInTheDocument(),
  );
});

test("quotes a shared file with its ID and path", async () => {
  serve([file]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText(/checklist/);
  await quoteFromMenu(rows()[0]);
  await userEvent.type(screen.getByRole("textbox"), "Check it");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      bot.bot_id,
      `<user_quote file_id="${fileId}" speaker="Bot" path="/work/checklist.md">checklist.md</user_quote>\nCheck it`,
    ),
  );
});

test("a second quote replaces the first", async () => {
  serve([reply, message(2, "bot_reply", "Second answer")]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("Second answer");
  await quoteFromMenu(rows()[0]);
  await quoteFromMenu(rows()[1]);
  expect(screen.getAllByRole("button", { name: /^Remove quote/ })).toHaveLength(1);
  await userEvent.type(screen.getByRole("textbox"), "ok");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(send).toHaveBeenCalledOnce());
  expect(send.mock.calls[0][1]).toBe(
    `<user_quote message_id="${id(2)}" speaker="Bot">Second answer</user_quote>\nok`,
  );
});

test("keeps the pending quote after reopening the chat until it is removed", async () => {
  serve([reply]);
  const first = render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  await quoteFromMenu(rows()[0]);
  first.unmount();
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: /^Remove quote/ }));
  expect(screen.queryByRole("button", { name: /^Remove quote/ })).not.toBeInTheDocument();
});

test("cannot send a quote without text, even with an attachment", async () => {
  const scope = `quotes:${crypto.randomUUID()}`;
  vi.spyOn(testDevice.access, "uploadStorageScope").mockResolvedValue(scope);
  await saveUploadRecord({
    scope,
    draft: `bot:${bot.bot_id}`,
    key: crypto.randomUUID(),
    uploadId: "u1",
    name: "a.txt",
    size: 1,
    sha256: "a".repeat(64),
  });
  vi.spyOn(testDevice.uploads, "uploadRequest").mockResolvedValue({
    upload_id: "u1",
    name: "a.txt",
    size: 1,
    sha256: "a".repeat(64),
    offset: 1,
    completed: true,
    path: "/device/a.txt",
    created_at: "2026-11-18T00:00:00Z",
  });
  serve([reply]);
  const send = sent();
  render(BotChatPage, { botId: bot.bot_id });
  const submit = await screen.findByRole("button", { name: "Send" });
  await waitFor(() => expect(submit).toBeEnabled());
  await quoteFromMenu(rows()[0]);
  expect(submit).toBeDisabled();
  await userEvent.keyboard("{Enter}");
  expect(send).not.toHaveBeenCalled();
  await userEvent.type(screen.getByRole("textbox"), "x");
  await fireEvent.click(submit);
  await waitFor(() => expect(send).toHaveBeenCalledOnce());
  expect(send.mock.calls[0][1]).toMatch(
    /^<user_quote message_id=[^]*<\/user_quote>\n<user_uploads [^]*<\/user_uploads>\nx$/,
  );
});

test("keeps the quote when sending fails", async () => {
  serve([reply]);
  vi.spyOn(testDevice.api, "sendBotMessage").mockRejectedValue(new Error("Offline"));
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  await quoteFromMenu(rows()[0]);
  await userEvent.type(screen.getByRole("textbox"), "hi");
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: /^Remove quote/ })).toBeInTheDocument();
});

test("copies the visible text of a user message", async () => {
  serve([
    message(
      3,
      "user_input",
      `<user_quote message_id="${id(1)}" speaker="Bot">x</user_quote>\nMy question`,
    ),
  ]);
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("My question");
  await fireEvent.contextMenu(rows()[0], { clientX: 10, clientY: 10 });
  await fireEvent.click(await screen.findByRole("menuitem", { name: "Copy" }));
  await waitFor(() => expect(writeText).toHaveBeenCalledWith("My question"));
});

test("keeps the browser menu for Shift+right-click and links", async () => {
  serve([message(1, "bot_reply", "See [docs](https://example.com).")]);
  render(BotChatPage, { botId: bot.bot_id });
  const link = await screen.findByRole("link", { name: /docs/ });
  expect(await fireEvent.contextMenu(link)).toBe(true);
  expect(await fireEvent.contextMenu(rows()[0], { shiftKey: true })).toBe(true);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
});

test("jumps to a loaded quoted message and highlights it", async () => {
  serve([
    reply,
    message(
      2,
      "user_input",
      `<user_quote message_id="${id(1)}" speaker="Bot">Renew</user_quote>\nOK`,
    ),
  ]);
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: /^Quote from Helper Bot/ }));
  await waitFor(() =>
    expect(document.querySelector("[data-highlighted]")).toHaveAttribute(
      "data-bot-entry-id",
      id(1),
    ),
  );
});

test("loads older history to reach a quoted message", async () => {
  const older = message(1, "user_input", "An old question");
  const fetch = serve(
    [
      message(
        5,
        "user_input",
        `<user_quote message_id="${id(1)}" speaker="User">An old question</user_quote>\nAgain`,
      ),
    ],
    { older: [older] },
  );
  render(BotChatPage, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: /^Quote from You/ }));
  await waitFor(() =>
    expect(document.querySelector("[data-highlighted]")).toHaveAttribute(
      "data-bot-entry-id",
      id(1),
    ),
  );
  expect(fetch).toHaveBeenCalledWith(
    bot.bot_id,
    expect.objectContaining({ before: "older" }),
    expect.anything(),
  );
});

test("reports a quoted message that no longer exists and stops offering the jump", async () => {
  serve(
    [
      message(
        5,
        "user_input",
        `<user_quote message_id="${id(1)}" speaker="Bot">gone</user_quote>\nStill?`,
      ),
    ],
    { missing: [id(1)] },
  );
  render(BotChatPage, { botId: bot.bot_id });
  const jump = await screen.findByRole("button", { name: /^Quote from Helper Bot/ });
  await fireEvent.click(jump);
  await waitFor(() => expect(toast.error).toHaveBeenCalledOnce());
  expect(jump).toBeDisabled();
  expect(document.querySelector("[data-highlighted]")).toBeNull();
});

test("opens the message menu from the keyboard and quotes with R", async () => {
  serve([reply]);
  render(BotChatPage, { botId: bot.bot_id });
  await screen.findByText("certificate");
  const row = rows()[0];
  row.focus();
  await userEvent.keyboard("r");
  expect(screen.getByRole("button", { name: /^Remove quote/ })).toBeInTheDocument();
  row.focus();
  await userEvent.keyboard("{Shift>}{F10}{/Shift}");
  expect(within(await screen.findByRole("menu")).getAllByRole("menuitem").length).toBeGreaterThan(
    0,
  );
});
