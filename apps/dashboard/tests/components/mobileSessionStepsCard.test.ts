import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { session } from "../pages/chat/fixtures";
import { testDevice } from "../deviceFixture";
import MobileSessionStepsCardHarness from "./MobileSessionStepsCardHarness.svelte";

test("opens a read step in the mobile file preview", async () => {
  const onOpen = vi.fn();
  const selected = session({
    session_id: "session-1",
    workspace: "/workspace/pontia",
  });
  render(MobileSessionStepsCardHarness, {
    props: {
      session: selected,
      onOpen,
      steps: [
        {
          id: "step-1",
          kind: "tool_call",
          title: "read",
          status: "completed",
          content: "src/app.ts",
          occurredAt: null,
          managedToolUse: {
            toolName: "read",
            input: { type: "read", path: "src/app.ts" },
          },
        },
      ],
    },
  });
  const user = userEvent.setup();

  await user.click(screen.getByRole("button", { name: "Show agent work steps" }));
  await user.click(screen.getByRole("button", { name: "Read 1 file" }));

  expect(onOpen).toHaveBeenCalledWith({
    device: testDevice,
    sessionId: "session-1",
    path: "/workspace/pontia/src/app.ts",
  });
});
