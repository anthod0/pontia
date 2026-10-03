import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import ChatRuler from "../../src/lib/components/session-chat/ChatRuler.svelte";
import type { TurnView } from "../../src/api/types";

function turn(overrides: Partial<TurnView>): TurnView {
  return {
    turn_id: "turn-root",
    session_id: "session-1",
    parent_turn_id: null,
    topology_status: "root",
    state: "completed",
    input: { summary: "Root question" },
    output: { summary: "Root answer" },
    failure: null,
    created_at: "2026-05-14T00:00:00Z",
    started_at: "2026-05-14T00:00:01Z",
    completed_at: "2026-05-14T00:00:02Z",
    metadata: {},
    ...overrides,
  };
}

const turns = [
  turn({}),
  turn({
    turn_id: "turn-current",
    parent_turn_id: "turn-root",
    topology_status: "linked",
    input: { summary: "Current branch question" },
    output: { summary: "Current branch answer" },
    created_at: "2026-05-14T00:01:00Z",
  }),
  turn({
    turn_id: "turn-other",
    parent_turn_id: "turn-root",
    topology_status: "linked",
    input: { summary: "Other branch question" },
    output: { summary: "Other branch answer" },
    created_at: "2026-05-14T00:02:00Z",
  }),
];

describe("ChatRuler", () => {
  test("renders user and assistant marks with hover summaries", async () => {
    const user = userEvent.setup();
    render(ChatRuler, {
      props: { turns, navigableTurnIds: turns.map((item) => item.turn_id) },
    });

    const marks = document.querySelectorAll("[data-chat-ruler-mark]");
    expect(marks).toHaveLength(6);
    const userMark = screen.getByRole("button", { name: "User message: Root question" });

    await user.hover(userMark);
    const summary = await screen.findByText("Root question");
    expect(summary).toBeVisible();
  });

  test("renders and navigates only current-lineage marks", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(ChatRuler, {
      props: {
        turns,
        treeMode: true,
        navigableTurnIds: ["turn-root", "turn-current"],
        onNavigate,
      },
    });

    expect(document.querySelectorAll("[data-chat-ruler-mark]")).toHaveLength(4);
    expect(
      screen.queryByRole("button", { name: "User message: Other branch question" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "User message: Current branch question" }));
    expect(onNavigate).toHaveBeenCalledTimes(1);
    expect(onNavigate).toHaveBeenCalledWith("turn-current", "user");
  });
});
