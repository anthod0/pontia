import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import ThoughtSummary from "../../src/lib/components/session-chat/ThoughtSummary.svelte";
import type { SessionChatThoughtStep } from "../../src/lib/session-chat/sessionChat";

function step(overrides: Partial<SessionChatThoughtStep> = {}): SessionChatThoughtStep {
  return {
    id: "thought-1",
    kind: "thinking",
    title: "Thinking",
    status: null,
    content: "Inspecting the project.",
    occurredAt: "2026-06-11T00:00:00Z",
    ...overrides,
  };
}

test.each([
  { title: "custom", managedToolUse: undefined, disclosure: "Show custom parameters" },
  {
    title: "Run command",
    managedToolUse: { tool_name: "bash", input: { type: "bash" as const, command: "echo input" } },
    disclosure: "Show Run command details",
  },
])(
  "$title keeps live output inside its disclosure and updates without duplication",
  async ({ title, managedToolUse, disclosure }) => {
    const user = userEvent.setup();
    const live = step({
      id: "live-call",
      kind: "tool_call",
      title,
      managedToolUse,
      content: "input",
      executionStatus: "running",
      processOutput: { text: "first chunk", truncated: false },
    });
    const { rerender } = render(ThoughtSummary, { props: { steps: [live], active: true } });
    await user.click(screen.getByRole("button", { name: "Show agent work steps" }));
    expect(screen.getByText("first chunk")).not.toBeVisible();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: disclosure }));
    expect(screen.getByText("first chunk")).toBeVisible();
    expect(screen.getByRole("status")).toBeVisible();
    const updated = {
      ...live,
      processOutput: { text: "first chunk and second", truncated: false },
    };
    await rerender({ steps: [updated], active: true });
    expect(screen.queryByText("first chunk")).not.toBeInTheDocument();
    expect(screen.getByText("first chunk and second")).toBeVisible();
    await rerender({
      steps: [
        {
          ...updated,
          executionStatus: "succeeded",
          result: { content: "first chunk and second", status: null, occurredAt: null },
        },
      ],
      active: true,
    });
    expect(screen.getAllByText("first chunk and second")).toHaveLength(1);
    await rerender({
      steps: [
        {
          ...updated,
          executionStatus: "failed",
          result: { content: "native failure", status: "error", occurredAt: null },
        },
      ],
      active: true,
    });
    expect(screen.getByText("native failure")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Output", exact: true }));
    expect(screen.getByText("first chunk and second")).toBeVisible();
  },
);

test.each(["read", "write", "edit"] as const)(
  "live %s calls stay grouped and preserve disclosure state as calls and results arrive",
  async (operation) => {
    const user = userEvent.setup();
    const fileCall = (id: string): SessionChatThoughtStep =>
      step({
        id,
        kind: "tool_call",
        title: `${operation} file`,
        content: `${id}.ts`,
        managedToolUse: { tool_name: operation, input: { type: operation, path: `${id}.ts` } },
        executionStatus: "running",
      });
    const first = fileCall("a");
    const { rerender } = render(ThoughtSummary, { props: { steps: [first], active: true } });
    await user.click(screen.getByRole("button", { name: "Show agent work steps" }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Show .*1 file details$/ }));
    expect(screen.getByRole("status")).toBeVisible();

    const completed: SessionChatThoughtStep = {
      ...first,
      executionStatus: "succeeded",
      result: { content: "first file result", status: null, occurredAt: null },
    };
    const second = fileCall("b");
    await rerender({ steps: [completed, second], active: true });
    expect(screen.getByRole("button", { name: /^Hide .*2 files details$/ })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getAllByText("first file result")).toHaveLength(1);
    expect(screen.getByText("first file result")).toBeVisible();
    expect(screen.getAllByRole("status")).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: /^Hide .*2 files details$/ }));
    await rerender({
      steps: [
        completed,
        {
          ...second,
          executionStatus: "failed",
          result: { content: "second file failed", status: "error", occurredAt: null },
        },
      ],
      active: false,
    });
    expect(screen.getByText("first file result")).not.toBeVisible();
    expect(screen.getByText("second file failed")).not.toBeVisible();
    expect(screen.queryByRole("status", { name: "" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Show .*2 files details$/ }));
    expect(screen.getByText("second file failed")).toBeVisible();
  },
);

test("completed work expands inline from its summary heading", async () => {
  const user = userEvent.setup();
  render(ThoughtSummary, {
    props: {
      steps: [
        step({ id: "thought-1", content: "Inspecting the project." }),
        step({
          id: "assistant-1",
          kind: "assistant",
          title: "Assistant update",
          content: "I found the **relevant** component.",
        }),
        step({
          id: "thought-2",
          kind: "tool_call",
          title: "Read file",
          content: "AGENTS.md",
          managedToolUse: { tool_name: "read", input: { type: "read", path: "AGENTS.md" } },
        }),
        step({ id: "unknown-1", kind: "tool_call", title: "Custom tool", content: "secret input" }),
        step({ id: "result-1", kind: "tool_result", title: "Read file", content: "File contents" }),
      ],
    },
  });

  const trigger = screen.getByRole("button", { name: "Show agent work steps" });
  expect(screen.getByText("Inspecting the project.")).not.toBeVisible();

  await user.click(trigger);

  expect(screen.getByRole("button", { name: "Hide agent work steps" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  expect(screen.getByLabelText("Thinking")).toBeInTheDocument();
  expect(screen.getByText("Inspecting the project.")).toBeInTheDocument();
  expect(screen.getByText("relevant")).toBeVisible();
  expect(screen.getByText("Read 1 file")).toBeInTheDocument();
  expect(screen.getByText("AGENTS.md")).not.toBeVisible();
  expect(screen.getByText("Custom tool")).toBeInTheDocument();
  expect(screen.getByText("secret input")).not.toBeVisible();
  expect(screen.getByText("File contents")).not.toBeVisible();
  await user.click(screen.getByRole("button", { name: "Show Read file result" }));
  expect(screen.getByText("File contents")).toBeVisible();

  await user.click(screen.getByRole("button", { name: "Show Custom tool parameters" }));

  expect(screen.getByRole("button", { name: "Hide Custom tool parameters" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  expect(screen.getByText("secret input")).toBeInTheDocument();
});

test("groups adjacent file operations by type and reveals their file lists", async () => {
  const user = userEvent.setup();
  render(ThoughtSummary, {
    props: {
      active: true,
      steps: [
        step({
          id: "read-1",
          kind: "tool_call",
          title: "Read file",
          content: "a.ts",
          managedToolUse: { tool_name: "read", input: { type: "read", path: "a.ts" } },
        }),
        step({
          id: "read-2",
          kind: "tool_call",
          title: "Read file",
          content: "b.ts",
          managedToolUse: { tool_name: "read", input: { type: "read", path: "b.ts" } },
        }),
        step({
          id: "write-1",
          kind: "tool_call",
          title: "Write file",
          content: "c.ts",
          managedToolUse: { tool_name: "write", input: { type: "write", path: "c.ts" } },
        }),
        step({
          id: "write-2",
          kind: "tool_call",
          title: "Write file",
          content: "d.ts",
          managedToolUse: { tool_name: "write", input: { type: "write", path: "d.ts" } },
        }),
        step({
          id: "edit-1",
          kind: "tool_call",
          title: "Edit file",
          content: "e.ts",
          managedToolUse: {
            tool_name: "edit",
            input: { type: "edit", path: "e.ts", edits_count: 1 },
          },
        }),
        step({
          id: "edit-2",
          kind: "tool_call",
          title: "Edit file",
          content: "f.ts",
          managedToolUse: {
            tool_name: "edit",
            input: { type: "edit", path: "f.ts", edits_count: 1 },
          },
        }),
      ],
    },
  });

  await user.click(screen.getByRole("button", { name: "Show agent work steps" }));

  expect(screen.getByText("Read 2 files")).toBeInTheDocument();
  expect(screen.getByText("Write 2 files")).toBeInTheDocument();
  expect(screen.getByText("Edit 2 files")).toBeInTheDocument();
  expect(screen.getByText("a.ts")).not.toBeVisible();

  await user.click(screen.getByRole("button", { name: "Show Read 2 files details" }));

  expect(screen.getByRole("button", { name: "Hide Read 2 files details" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  expect(screen.getByText("a.ts")).toBeInTheDocument();
  expect(screen.getByText("b.ts")).toBeInTheDocument();
});

test("run command reveals its command from a nested disclosure", async () => {
  const user = userEvent.setup();
  render(ThoughtSummary, {
    props: {
      active: true,
      steps: [
        step({
          id: "bash-1",
          kind: "tool_call",
          title: "Run command",
          content: "pnpm test",
          managedToolUse: { tool_name: "bash", input: { type: "bash", command: "pnpm test" } },
        }),
      ],
    },
  });

  await user.click(screen.getByRole("button", { name: "Show agent work steps" }));

  const trigger = screen.getByRole("button", { name: "Show Run command details" });
  expect(document.querySelector(".command-code")).not.toBeVisible();

  await user.click(trigger);

  expect(screen.getByRole("button", { name: "Hide Run command details" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  expect(document.querySelector(".command-code")).toBeVisible();
  expect(document.querySelector(".command-code")).toHaveTextContent("pnpm test");
});

test("streaming updates and activity changes preserve the user disclosure state", async () => {
  const user = userEvent.setup();
  const { rerender } = render(ThoughtSummary, {
    props: { steps: [step()], active: false },
  });

  await rerender({ active: true });
  expect(screen.getByRole("button", { name: "Show agent work steps" })).toHaveAttribute(
    "aria-expanded",
    "false",
  );

  await user.click(screen.getByRole("button", { name: "Show agent work steps" }));
  await rerender({ steps: [step({ content: "Updated thinking." })] });
  expect(screen.getByText("Updated thinking.")).toBeVisible();

  await rerender({ active: false });
  expect(screen.getByRole("button", { name: "Hide agent work steps" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );

  await rerender({ active: true });
  expect(screen.getByRole("button", { name: "Hide agent work steps" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );

  await user.click(screen.getByRole("button", { name: "Hide agent work steps" }));
  await rerender({ steps: [step({ content: "More thinking." })] });
  expect(screen.getByRole("button", { name: "Show agent work steps" })).toHaveAttribute(
    "aria-expanded",
    "false",
  );
  expect(screen.getByText("More thinking.")).not.toBeVisible();
});

test("native errors and question answers remain inspectable without assistant text", async () => {
  const user = userEvent.setup();
  render(ThoughtSummary, {
    props: {
      steps: [
        step({
          id: "error",
          kind: "tool_result",
          title: "Native error",
          status: "error",
          content: "workspace routing discovery failed",
        }),
        step({
          id: "answers",
          kind: "tool_result",
          title: "Questions and answers",
          content: "Questions: Color?\nAnswers: Green",
        }),
      ],
    },
  });
  await user.click(screen.getByRole("button", { name: "Show agent work steps" }));
  await user.click(screen.getByRole("button", { name: "Show Native error result" }));
  expect(screen.getByText("workspace routing discovery failed")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Show Questions and answers result" }));
  expect(screen.getByText(/Questions: Color.*Answers: Green/s)).toBeVisible();
});

test.each(["exec finished successfully", '{"count":9007199254740993,"count":2}\n'])(
  "preserves the exact native tool result %j",
  async (content) => {
    const user = userEvent.setup();
    render(ThoughtSummary, {
      props: { steps: [step({ kind: "tool_result", title: "exec", content })] },
    });
    await user.click(screen.getByRole("button", { name: "Show agent work steps" }));
    await user.click(screen.getByRole("button", { name: "Show exec result" }));
    const output = screen.getByText(content, { normalizer: (value) => value });
    expect(output).toBeVisible();
  },
);
