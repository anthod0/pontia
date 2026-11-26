import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import TurnDuration from "../../src/lib/components/session-chat/TurnDuration.svelte";

test("reports running and completed turns with the matching duration semantics", async () => {
  const startedAt = "2026-11-27T00:00:00Z";
  const view = render(TurnDuration, {
    turn: { state: "running", started_at: startedAt, completed_at: null },
  });

  expect(screen.getByRole("timer")).toHaveAttribute("aria-live", "off");

  await view.rerender({
    turn: { state: "completed", started_at: startedAt, completed_at: "2026-11-27T00:00:05Z" },
  });

  expect(screen.getByRole("status")).toHaveAttribute("aria-live", "polite");
});
