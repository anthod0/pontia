import { render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import BotChatPage from "../../src/pages/BotChatPage.svelte";

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(56);
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("role") === "log" ? 640 : 56;
  });
});
afterEach(() => vi.restoreAllMocks());

test("renders Bot replies as markdown while keeping user messages as plain text", async () => {
  const botId = crypto.randomUUID();
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot: {
      bot_id: botId,
      name: "Bot",
      description: null,
      avatar_shape: "classic",
      avatar_color: "blue",
      client_type: "pi",
      client_key: "default",
      model: "test",
      working_on: null,
    },
    entries: [
      {
        id: "msg-user",
        kind: "user_input",
        content: "Keep [literal](https://example.com/user) raw",
        status: "normal",
        created_at: "2026-11-20T00:00:00Z",
      },
      {
        id: "msg-bot",
        kind: "bot_reply",
        content: [
          "See [docs](https://example.com/docs).",
          "",
          "```ts",
          "const answer = 42;",
          "```",
          "",
          '<script>alert("unsafe")</script>',
        ].join("\n"),
        status: "normal",
        created_at: "2026-11-20T00:00:01Z",
      },
    ],
    next_before: null,
    notice_session_id: null,
    generation_notice: null,
    processing: false,
  });

  const { container } = render(BotChatPage, { botId });

  const link = await screen.findByRole("link", { name: /docs/i });
  await waitFor(() => {
    expect(link).toHaveAttribute("href", "https://example.com/docs");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });
  await waitFor(() => {
    expect(container.querySelector("[data-code-block] code")).toHaveTextContent(
      "const answer = 42;",
    );
  });
  expect(container.querySelector("script")).not.toBeInTheDocument();

  expect(screen.queryByRole("link", { name: /literal/i })).not.toBeInTheDocument();
  expect(screen.getByText("Keep [literal](https://example.com/user) raw")).toBeInTheDocument();
});

function chatWith(processing: boolean, count = 1) {
  const botId = crypto.randomUUID();
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot: {
      bot_id: botId,
      name: "Bot",
      description: null,
      avatar_shape: "classic",
      avatar_color: "blue",
      client_type: "pi",
      client_key: "default",
      model: "test",
      working_on: null,
    },
    entries: Array.from({ length: count }, (_, index) => ({
      id: `msg-${index}`,
      kind: "user_input",
      content: index === 0 ? "Hello" : `Message ${index}`,
      status: "normal",
      created_at: new Date(index).toISOString(),
    })),
    next_before: null,
    notice_session_id: null,
    generation_notice: null,
    processing,
  });
  render(BotChatPage, { botId });
  return screen.findByRole("log");
}

test("shows a status while the Bot processes an unanswered message", async () => {
  await chatWith(true);
  expect(screen.getByRole("status").querySelector("svg.bot")).toHaveClass("classic");
});

test("shows no status when the Bot is idle", async () => {
  await chatWith(false);
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});

test("mounts only the visible window of a full Bot history page", async () => {
  const log = await chatWith(false, 50);
  await waitFor(() => {
    const rows = log.querySelectorAll("[data-bot-entry-id]");
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(30);
  });
});
