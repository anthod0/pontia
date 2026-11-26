import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { Bot, BotMessage } from "../../src/api/types";
import BotContextList from "../../src/components/bots/BotContextList.svelte";
import { parseQuotedMessage, quotedMessage } from "../../src/lib/botQuote";

const bot: Bot = {
  bot_id: "atlas",
  name: "Atlas",
  description: null,
  avatar_shape: "mochi",
  avatar_color: "aqua",
  client_type: "codex",
  client_key: "default",
  model: "gpt-test",
  reasoning_effort: null,
};

const longReply: BotMessage = {
  id: "bmsg_018f18f0-4f42-7c28-98cb-7150d9000001",
  kind: "bot_reply",
  content: "# Release check\n\nOne\n\nTwo\n\nThree\n\nFour\n\nFive\n\nSix",
  status: "normal",
  created_at: "2026-11-26T06:12:00Z",
};

const current: BotMessage = {
  id: "bmsg_018f18f0-4f42-7c28-98cb-7150d9000002",
  kind: "user_input",
  content: "What should I do next?",
  status: "normal",
  created_at: "2026-11-26T06:17:00Z",
};

const originalScrollHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollHeight");
const originalScrollIntoView = HTMLElement.prototype.scrollIntoView;
const originalGetComputedStyle = window.getComputedStyle;

beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
    configurable: true,
    get: () => 200,
  });
  HTMLElement.prototype.scrollIntoView = vi.fn();
  window.getComputedStyle = ((element: Element) => {
    const styles = originalGetComputedStyle(element);
    return new Proxy(styles, {
      get: (target, property) =>
        property === "lineHeight" ? "20px" : Reflect.get(target, property),
    });
  }) as typeof window.getComputedStyle;
});

afterEach(() => {
  if (originalScrollHeight)
    Object.defineProperty(HTMLElement.prototype, "scrollHeight", originalScrollHeight);
  else delete (HTMLElement.prototype as { scrollHeight?: number }).scrollHeight;
  HTMLElement.prototype.scrollIntoView = originalScrollIntoView;
  window.getComputedStyle = originalGetComputedStyle;
});

test("expands a truncated message from its row and only collapses it from the header or control", async () => {
  render(BotContextList, {
    props: { bot, selected: current.id, messages: [longReply, current], onjump: vi.fn() },
  });

  const showMore = await screen.findByRole("button", { name: "Show more" });
  const item = showMore.closest("li")!;
  await fireEvent.click(item);
  const showLess = await screen.findByRole("button", { name: "Show less" });
  expect(showLess).toHaveAttribute("aria-expanded", "true");

  await fireEvent.click(within(item).getByRole("heading", { name: "Release check" }));
  expect(screen.getByRole("button", { name: "Show less" })).toBeInTheDocument();

  await fireEvent.click(item.querySelector("header")!);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Show more" })).toHaveAttribute(
      "aria-expanded",
      "false",
    ),
  );
});

test("locates a message through the supplied chat jump behavior", async () => {
  const onjump = vi.fn();
  render(BotContextList, {
    props: { bot, selected: current.id, messages: [longReply, current], onjump },
  });

  const replyItem = (await screen.findByRole("button", { name: "Show more" })).closest("li")!;
  await fireEvent.click(within(replyItem).getByRole("button", { name: /Show Atlas's message/ }));
  expect(onjump).toHaveBeenCalledWith(longReply.id);
});

test("keeps quote navigation inside the context list when the target is present", async () => {
  const quoted: BotMessage = {
    ...current,
    content: quotedMessage(
      {
        kind: "message",
        messageId: longReply.id,
        speaker: "Bot",
        text: "Release check",
        excerpt: true,
      },
      current.content,
    ),
  };
  const onjump = vi.fn();
  expect(parseQuotedMessage(quoted.content).quote).not.toBeNull();
  render(BotContextList, {
    props: { bot, selected: quoted.id, messages: [longReply, quoted], onjump },
  });

  const quote = await screen.findByRole("button", { name: /Quote from Atlas/ });
  await fireEvent.click(quote);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Show less" })).toHaveAttribute(
      "aria-expanded",
      "true",
    ),
  );
  expect(onjump).not.toHaveBeenCalled();
});

test("renders the current Bot message as Markdown without truncating it", async () => {
  const markdown: BotMessage = {
    ...longReply,
    content: "# Release check\n\n| Item | Owner |\n| --- | --- |\n| Migration | You |",
  };
  render(BotContextList, {
    props: { bot, selected: markdown.id, messages: [markdown], onjump: vi.fn() },
  });

  expect(await screen.findByRole("heading", { name: "Release check" })).toBeInTheDocument();
  expect(screen.getByRole("table")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Show more" })).not.toBeInTheDocument();
});
