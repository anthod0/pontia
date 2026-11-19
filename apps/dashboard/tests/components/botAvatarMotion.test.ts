import { render } from "@testing-library/svelte";
import { expect, test } from "vitest";
import BotAvatar from "../../src/components/bots/BotAvatar.svelte";

test("stays still without a working state", () => {
  const { container } = render(BotAvatar, { shape: "blob", color: "brown" });
  const svg = container.querySelector("svg")!;
  expect(svg).not.toHaveClass("bot");
  expect(svg.querySelector("animate, animateTransform")).toBeNull();
});

test("idles until the Bot starts working, then wakes up", async () => {
  const { container, rerender } = render(BotAvatar, {
    shape: "blob",
    color: "brown",
    working: false,
  });
  const svg = container.querySelector("svg")!;
  expect(svg).toHaveClass("bot", "blob", "idle");
  expect(svg.querySelectorAll("animate, animateTransform")).toHaveLength(2);

  await rerender({ shape: "blob", color: "brown", working: true });
  expect(svg).toHaveClass("waking");
});
