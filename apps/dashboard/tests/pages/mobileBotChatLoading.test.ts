import "../shallowRouting";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test, vi } from "vitest";
import type { Bot } from "../../src/api/types";
import type { DashboardHost } from "../../src/device/hostContext";
import { writeCachedBotTimeline, removeCachedBotTimeline } from "../../src/lib/botTimelineCache";
import { createLocalHost } from "../../src/modes/local/host.svelte";
import { testDevice } from "../deviceFixture";
import MobileBotChatHarness from "./MobileBotChatHarness.svelte";

afterEach(async () => {
  vi.restoreAllMocks();
  await removeCachedBotTimeline(testDevice.id, "review");
});

const bot: Bot = {
  bot_id: "review",
  name: "Reviewer",
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "test",
  reasoning_effort: null,
};

function connectingHost(): DashboardHost {
  const local = createLocalHost(() => testDevice);
  return {
    ...local,
    mode: "public",
    connecting: true,
    activeDevice: {
      id: "office-mac",
      handle: "office-mac",
      name: "Office Mac",
      state: "connecting",
      runtime: testDevice,
    },
  };
}

test("a connecting device shows the saved Bot chat read-only", async () => {
  await writeCachedBotTimeline({
    deviceId: testDevice.id,
    botId: "review",
    bot,
    fileLinkSession: null,
    entries: [
      {
        id: "saved",
        kind: "bot_reply",
        status: "normal",
        created_at: "2026-10-01T01:00:00Z",
        content: "Saved reply",
      },
    ],
  });
  vi.spyOn(testDevice.api, "getBotMessages").mockReturnValue(new Promise(() => {}));
  render(MobileBotChatHarness, { botId: "review", host: connectingHost() });
  // The header and composer come from the saved Bot while its latest page is still pending.
  expect(await screen.findByRole("button", { name: "Reviewer" })).toBeInTheDocument();
  expect(screen.getByText("Connecting to Office Mac… showing saved messages")).toBeInTheDocument();
  await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "hello");
  expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
});

test("without saved messages the mobile Bot chat skeleton holds the header and composer places", () => {
  vi.spyOn(testDevice.api, "getBotMessages").mockReturnValue(new Promise(() => {}));
  render(MobileBotChatHarness, { botId: "review" });
  const skeleton = screen.getByRole("status", { name: "Loading conversation" });
  expect(skeleton).toHaveAttribute("data-bot-conversation-skeleton");
});

test("an online device sends from the Bot chat", async () => {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [],
    open_todo_count: 0,
    file_link_session: null,
    notice_session_id: null,
    generation_notice: null,
    next_before: null,
    status: { kind: "idle" },
  });
  render(MobileBotChatHarness, { botId: "review" });
  await userEvent.type(await screen.findByRole("textbox", { name: "Message" }), "hello");
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" })).toBeEnabled());
  expect(screen.queryByText(/showing saved messages/)).toBeNull();
});
