import "../shallowRouting";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test, vi } from "vitest";
import type { BotMessagePage } from "../../src/api/types";
import { testDevice } from "../deviceFixture";
import MobileBotChatHarness from "./MobileBotChatHarness.svelte";

afterEach(() => vi.restoreAllMocks());

const id = "bmsg_01234567-89ab-7abc-8def-000000000001";
const message = {
  id,
  kind: "bot_reply",
  status: "normal",
  created_at: "2026-10-01T01:00:00Z",
  content: "Review this",
  todo: { state: "open" },
} as const;
const data: BotMessagePage = {
  bot: {
    bot_id: "review",
    name: "Reviewer",
    description: null,
    avatar_shape: "classic",
    avatar_color: "blue",
    client_type: "pi",
    client_key: "default",
    model: "test",
    reasoning_effort: null,
  },
  entries: [message],
  open_todo_count: 1,
  file_link_session: null,
  notice_session_id: null,
  generation_notice: null,
  next_before: null,
  status: { kind: "idle" },
};

function openChat() {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue(data);
  vi.spyOn(testDevice.access, "uploadStorageScope").mockResolvedValue("mobile-draft");
  render(MobileBotChatHarness, { botId: "review" });
}

test("selecting /reload from the mobile command menu reloads the Bot and clears the draft", async () => {
  const anchor = new DOMRect(56, 760, 278, 44);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(anchor);
  vi.spyOn(HTMLElement.prototype, "getClientRects").mockReturnValue([
    anchor,
  ] as unknown as DOMRectList);
  vi.spyOn(document.documentElement, "clientWidth", "get").mockReturnValue(390);
  vi.spyOn(document.documentElement, "clientHeight", "get").mockReturnValue(844);
  const reload = vi.spyOn(testDevice.api, "reloadBot").mockResolvedValue();
  openChat();
  const input = await screen.findByRole("textbox", { name: "Message" });
  await userEvent.type(input, "/");
  await userEvent.click(await screen.findByRole("option", { name: /\/reload/ }));
  await waitFor(() => expect(reload).toHaveBeenCalledExactlyOnceWith("review"));
  await waitFor(() => expect(input.textContent).toBe(""));
});

test("quoting a todo returns to the current Bot draft and sends both together", async () => {
  vi.spyOn(testDevice.api, "getBotTodos").mockResolvedValue({
    open: [message],
    recently_resolved: [],
  });
  const send = vi
    .spyOn(testDevice.api, "sendBotMessage")
    .mockResolvedValue({ ...message, id: "new", kind: "user_input" });
  openChat();
  const input = await screen.findByRole("textbox", { name: "Message" });
  await userEvent.type(input, "Looks good");
  await fireEvent.click(screen.getByRole("button", { name: "Todo · 1" }));
  await fireEvent.click(
    await within(await screen.findByRole("dialog")).findByRole("button", { name: "Quote" }),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(input).toHaveTextContent("Looks good");
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" })).toBeEnabled());
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      "review",
      `<user_quote message_id="${id}" speaker="Bot">Review this</user_quote>\nLooks good`,
    ),
  );
});
