import { render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import BotChatPage from "../../src/pages/BotChatPage.svelte";

afterEach(() => vi.restoreAllMocks());

test("renders Bot replies as markdown while keeping user messages as plain text", async () => {
  const botId = crypto.randomUUID();
  vi.spyOn(testDevice.api, "getBotChat").mockResolvedValue({
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
    messages: [
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
    notices: [],
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
