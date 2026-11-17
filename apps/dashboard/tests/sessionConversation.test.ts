import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { expect, test, vi } from "vitest";
import SessionConversation from "../src/lib/components/session-chat/SessionConversation.svelte";
import type { TurnView } from "../src/api/types";
import type { SessionChatMessage } from "../src/lib/session-chat/sessionChat";
import { timelineItemsToChatMessages } from "../src/lib/session-chat/sessionChat";
import type { TurnTimelineItem } from "../src/api/types";

test("checkpoints preserve their native positions inside and between Turns without becoming messages", () => {
  const item = (id: string, kind: string, turnId: string | null): TurnTimelineItem => ({
    item_id: id,
    kind,
    turn_id: turnId,
    role: null,
    status: null,
    title: null,
    occurred_at: null,
    content_preview: id,
  });
  const timelineItems = [
    item("user", "user", "turn-1"),
    item("before", "assistant", "turn-1"),
    item("inside", "context_compaction", "turn-1"),
    item("after", "assistant", "turn-1"),
    item("gap", "context_compaction", null),
    item("next", "user", "turn-2"),
    item("tail", "context_compaction", null),
  ];
  const { container } = render(SessionConversation, {
    messages: timelineItemsToChatMessages(timelineItems, true),
    timelineItems,
  });
  const entries = [
    ...container.querySelectorAll<HTMLElement>("[data-chat-message-id], [data-context-compaction]"),
  ];
  expect(
    entries.map((element) => element.dataset.chatMessageId ?? element.dataset.contextCompaction),
  ).toEqual(["user", "before", "inside", "after", "gap", "next", "tail"]);
  expect(container.querySelectorAll("[data-chat-message-id]")).toHaveLength(4);
  const checkpoint = within(
    container.querySelector('[data-context-compaction="inside"]') as HTMLElement,
  );
  expect(checkpoint.queryByRole("button")).not.toBeInTheDocument();
  expect(checkpoint.queryByText("inside")).not.toBeInTheDocument();
});

test("a checkpoint is visible when no Pontia Turn exists yet", () => {
  const { container } = render(SessionConversation, {
    messages: [],
    timelineItems: [
      {
        item_id: "standalone",
        kind: "context_compaction",
        turn_id: null,
        role: null,
        title: null,
        status: null,
        occurred_at: null,
        content_preview: null,
      },
    ],
  });
  expect(container.querySelector('[data-context-compaction="standalone"]')).toBeInTheDocument();
  expect(container.querySelector("[data-chat-turn]")).not.toBeInTheDocument();
});

class TestIntersectionObserver implements IntersectionObserver {
  static instances: TestIntersectionObserver[] = [];

  readonly root: Element | Document | null = null;
  readonly rootMargin = "0px";
  readonly thresholds = [0];
  private observedElement: Element | null = null;

  constructor(private readonly callback: IntersectionObserverCallback) {
    TestIntersectionObserver.instances.push(this);
  }

  observe(element: Element): void {
    this.observedElement = element;
  }

  unobserve(): void {}

  disconnect(): void {}

  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }

  trigger(isIntersecting: boolean): void {
    if (!this.observedElement) return;
    this.callback(
      [{ isIntersecting, target: this.observedElement } as IntersectionObserverEntry],
      this,
    );
  }
}

function installIntersectionObserverMock(): void {
  TestIntersectionObserver.instances = [];
  Object.defineProperty(window, "IntersectionObserver", {
    configurable: true,
    writable: true,
    value: TestIntersectionObserver,
  });
  Object.defineProperty(globalThis, "IntersectionObserver", {
    configurable: true,
    writable: true,
    value: TestIntersectionObserver,
  });
}

function durationTurn(overrides: Partial<TurnView>): TurnView {
  return {
    turn_id: "turn-1",
    session_id: "session-1",
    parent_turn_id: null,
    topology_status: "root",
    state: "completed",
    input: null,
    output: null,
    failure: null,
    created_at: "2026-06-11T00:00:00Z",
    started_at: "2026-06-11T00:00:00Z",
    completed_at: "2026-06-11T00:00:10Z",
    metadata: {},
    ...overrides,
  };
}

const messages: SessionChatMessage[] = [
  {
    id: "message-1",
    role: "user",
    content: "Please inspect the repo.",
    status: "sent",
  },
  {
    id: "message-2",
    role: "assistant",
    content: "I will inspect it now.",
    status: "sent",
  },
];

test("conversation groups each Turn’s user and assistant messages with its status", () => {
  const firstTurn: SessionChatMessage[] = messages.map((message) => ({
    ...message,
    turnId: "turn-1",
    createdAt: "2026-06-11T00:00:00Z",
  }));
  const secondTurn: SessionChatMessage[] = [
    {
      id: "turn-2:user",
      turnId: "turn-2",
      role: "user",
      content: "Next question.",
      status: "sent",
      createdAt: "2026-06-11T00:01:00Z",
    },
    {
      id: "turn-2:assistant",
      turnId: "turn-2",
      role: "assistant",
      content: "Streaming reply.",
      status: "pending",
      createdAt: "2026-06-11T00:01:00Z",
    },
  ];
  render(SessionConversation, {
    props: {
      messages: [...firstTurn, ...secondTurn],
      sessionState: "busy",
      activeTurnId: "turn-2",
    },
  });
  const turns = document.querySelectorAll("[data-chat-turn]");
  expect(turns).toHaveLength(2);
  expect(turns[0]).toHaveAttribute("data-chat-turn-id", "turn-1");
  expect(turns[0]).toContainElement(screen.getByText("Please inspect the repo."));
  expect(turns[0]).toContainElement(screen.getByText("I will inspect it now."));
  expect(turns[1]).toHaveAttribute("data-chat-turn-id", "turn-2");
  expect(turns[1]).toContainElement(screen.getByText("Next question."));
  expect(turns[1]).toContainElement(screen.getByText("Streaming reply."));
  expect(turns[1]).toContainElement(screen.getByRole("timer", { name: "Turn running time" }));
});

test("conversation shows duration for an earlier completed Turn", () => {
  const firstTurnMessages = messages.map((message) => ({
    ...message,
    turnId: "turn-completed",
    createdAt: "2026-06-11T00:00:00Z",
  }));
  const activeTurnMessages: SessionChatMessage[] = [
    {
      id: "turn-running:user",
      turnId: "turn-running",
      role: "user",
      content: "Continue.",
      status: "sent",
      createdAt: "2026-06-11T00:02:00Z",
    },
    {
      id: "turn-running:assistant",
      turnId: "turn-running",
      role: "assistant",
      content: "In progress",
      status: "pending",
      createdAt: "2026-06-11T00:02:00Z",
    },
  ];

  render(SessionConversation, {
    props: {
      messages: [...firstTurnMessages, ...activeTurnMessages],
      turns: [
        durationTurn({
          turn_id: "turn-completed",
          started_at: "2026-06-11T00:00:00Z",
          completed_at: "2026-06-11T00:01:30Z",
        }),
        durationTurn({
          turn_id: "turn-running",
          state: "running",
          started_at: "2026-06-11T00:02:00Z",
          completed_at: null,
        }),
      ],
      sessionState: "busy",
      activeTurnId: "turn-running",
    },
  });

  const completedDuration = screen.getByRole("status", { name: "Turn completed duration" });
  expect(completedDuration.closest("[data-chat-turn]")).toHaveAttribute(
    "data-chat-turn-id",
    "turn-completed",
  );
  expect(completedDuration.querySelector("time")).toHaveAttribute("datetime", "PT90S");
});

test("conversation updates the running Turn duration in the Turn footer while output is streaming", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-06-11T00:02:00Z"));
  const runningMessages = messages.map((message) => ({
    ...message,
    turnId: "turn-running",
    createdAt: "2026-06-11T00:00:00Z",
    status: message.role === "assistant" ? ("pending" as const) : message.status,
  }));

  const { rerender } = render(SessionConversation, {
    props: {
      messages: runningMessages,
      turns: [
        durationTurn({
          turn_id: "turn-running",
          state: "running",
          started_at: "2026-06-11T00:00:00Z",
          completed_at: null,
        }),
      ],
      sessionState: "busy",
      activeTurnId: "turn-running",
    },
  });

  const timer = screen.getByRole("timer", { name: "Turn running time" });
  expect(timer.querySelector("time")).toHaveAttribute("datetime", "PT120S");
  expect(timer.closest("[data-chat-turn-footer]")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /copy assistant reply/i })).not.toBeInTheDocument();

  await vi.advanceTimersByTimeAsync(2000);
  expect(timer.querySelector("time")).toHaveAttribute("datetime", "PT122S");

  await rerender({
    messages: runningMessages.map((message) => ({ ...message, status: "sent" as const })),
    turns: [
      durationTurn({
        turn_id: "turn-running",
        completed_at: "2026-06-11T00:02:03Z",
      }),
    ],
    sessionState: "idle",
    activeTurnId: null,
  });
  await vi.advanceTimersByTimeAsync(5000);

  const duration = screen.getByRole("status", { name: "Turn completed duration" });
  expect(duration.querySelector("time")).toHaveAttribute("datetime", "PT123S");
  expect(screen.queryByRole("timer")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /copy assistant reply/i })).toBeInTheDocument();
  vi.useRealTimers();
});
test("conversation renders a steered Turn in consumption order with one footer", () => {
  const turnId = "turn-steered";
  const at = (seconds: number) => `2026-06-11T00:00:${String(seconds).padStart(2, "0")}Z`;
  render(SessionConversation, {
    props: {
      sessionState: "idle",
      turns: [durationTurn({ turn_id: turnId })],
      messages: [
        { id: "u1", turnId, role: "user", content: "Start.", status: "sent", createdAt: at(0) },
        {
          id: "a1",
          turnId,
          role: "assistant",
          content: "First update.",
          status: "sent",
          createdAt: at(1),
        },
        {
          id: "c1:working",
          turnId,
          role: "assistant",
          content: "",
          status: "sent",
          createdAt: at(2),
          thoughtSteps: [
            {
              id: "c1",
              kind: "tool_call",
              title: "edit",
              status: null,
              content: "a.ts",
              occurredAt: at(2),
            },
          ],
        },
        {
          id: "u2",
          turnId,
          role: "user",
          content: "Use six days.",
          status: "sent",
          createdAt: at(3),
        },
        {
          id: "a2",
          turnId,
          role: "assistant",
          content: "Recorded.",
          status: "sent",
          createdAt: at(4),
        },
      ],
    },
  });

  const turn = document.querySelector(`[data-chat-turn-id="${turnId}"]`) as HTMLElement;
  const order = [...turn.querySelectorAll<HTMLElement>("[data-chat-message-id]")].map(
    (element) => element.dataset.chatMessageId,
  );
  expect(order).toEqual(["u1", "a1", "c1:working", "u2", "a2"]);
  expect(screen.getByText("First update.")).toBeVisible();
  expect(turn.querySelectorAll("[data-chat-turn-footer]")).toHaveLength(1);
  expect(
    screen
      .getByText("Recorded.")
      .compareDocumentPosition(turn.querySelector("[data-chat-turn-footer]") as Node) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(screen.getAllByRole("button", { name: /copy assistant reply/i })).toHaveLength(1);
});
test("conversation shows the running timer only on the latest Turn", () => {
  render(SessionConversation, {
    props: {
      sessionState: "busy",
      messages: [
        ...messages,
        {
          id: "message-3",
          role: "user",
          content: "Check one more file.",
          status: "sent",
        },
        {
          id: "message-4",
          role: "assistant",
          content: "I checked it.",
          status: "sent",
        },
      ],
    },
  });

  expect(screen.getAllByRole("timer", { name: "Turn running time" })).toHaveLength(1);
});
test("conversation hides the agent status component while the session is idle", () => {
  render(SessionConversation, { props: { sessionState: "idle", messages } });

  expect(screen.queryByLabelText(/agent status/i)).not.toBeInTheDocument();
  expect(screen.queryByText("Agent idle")).not.toBeInTheDocument();
});

test("conversation renders exited status after the conversation", () => {
  render(SessionConversation, { props: { sessionState: "exited", messages } });

  expect(screen.queryByLabelText(/agent status/i)).not.toBeInTheDocument();
  const bottomStatus = screen.getByText("session exited · send a message to resume");
  expect(
    screen.getByText("I will inspect it now.").compareDocumentPosition(bottomStatus) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
});

test("conversation renders interrupted status after the conversation", () => {
  render(SessionConversation, { props: { sessionState: "interrupted", messages } });

  expect(screen.queryByLabelText(/agent status/i)).not.toBeInTheDocument();

  const bottomStatus = screen.getByText("session interrupted");
  expect(
    screen.getByText("I will inspect it now.").compareDocumentPosition(bottomStatus) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
});

test("conversation copies assistant reply content with the http-compatible fallback", async () => {
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
  document.execCommand = vi.fn().mockReturnValue(true);
  const execCommand = vi.mocked(document.execCommand);

  render(SessionConversation, { props: { messages } });

  const copyButton = screen.getByRole("button", { name: /copy assistant reply/i });
  await fireEvent.click(copyButton);

  expect(execCommand).toHaveBeenCalledWith("copy");
  expect(screen.getByRole("button", { name: /assistant reply copied/i })).toBeInTheDocument();
});

test("conversation waits to observe earlier history until history observer is enabled", async () => {
  installIntersectionObserverMock();
  const onLoadMoreHistory = vi.fn();

  const { rerender } = render(SessionConversation, {
    props: { messages, hasMoreHistory: true, historyObserverEnabled: false, onLoadMoreHistory },
  });

  expect(TestIntersectionObserver.instances).toHaveLength(0);
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();

  await rerender({
    messages,
    hasMoreHistory: true,
    historyObserverEnabled: true,
    onLoadMoreHistory,
  });

  await waitFor(() => expect(TestIntersectionObserver.instances.length).toBeGreaterThan(0));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();
});

test("conversation waits for an extra upward pull after the top sentinel intersects before loading history", async () => {
  installIntersectionObserverMock();
  const onLoadMoreHistory = vi.fn();
  render(SessionConversation, {
    props: { messages, hasMoreHistory: true, historyObserverEnabled: true, onLoadMoreHistory },
  });

  expect(screen.queryByRole("button", { name: /load earlier messages/i })).not.toBeInTheDocument();
  await waitFor(() => expect(TestIntersectionObserver.instances.length).toBeGreaterThan(0));
  TestIntersectionObserver.instances.at(-1)?.trigger(false);
  window.dispatchEvent(new WheelEvent("wheel", { deltaY: -160 }));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();

  TestIntersectionObserver.instances.at(-1)?.trigger(true);
  expect(await screen.findByText("Keep scrolling up to load earlier messages")).toBeInTheDocument();
  expect(onLoadMoreHistory).not.toHaveBeenCalled();

  window.dispatchEvent(new WheelEvent("wheel", { deltaY: -64 }));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();
  window.dispatchEvent(new WheelEvent("wheel", { deltaY: -64 }));

  await waitFor(() => expect(onLoadMoreHistory).toHaveBeenCalledTimes(1));
});

test("conversation preserves the visible history anchor after prepending earlier messages", async () => {
  installIntersectionObserverMock();
  const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  const requestAnimationFrame = vi
    .spyOn(window, "requestAnimationFrame")
    .mockImplementation((callback) => {
      callback(0);
      return 1;
    });
  Object.defineProperty(window, "scrollY", { configurable: true, value: 120 });
  Object.defineProperty(document.documentElement, "scrollHeight", {
    configurable: true,
    value: 1000,
  });
  const onLoadMoreHistory = vi.fn(async () => {
    Object.defineProperty(document.documentElement, "scrollHeight", {
      configurable: true,
      value: 1400,
    });
  });

  render(SessionConversation, {
    props: { messages, hasMoreHistory: true, historyObserverEnabled: true, onLoadMoreHistory },
  });

  await waitFor(() => expect(TestIntersectionObserver.instances.length).toBeGreaterThan(0));
  TestIntersectionObserver.instances.at(-1)?.trigger(true);
  window.dispatchEvent(new WheelEvent("wheel", { deltaY: -120 }));

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 520 }));
  expect(onLoadMoreHistory).toHaveBeenCalledTimes(1);
  scrollTo.mockRestore();
  requestAnimationFrame.mockRestore();
});

test("conversation does not scroll the document to the bottom on initial render", async () => {
  const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 800 });
  Object.defineProperty(window, "scrollY", { configurable: true, value: 3200 });
  Object.defineProperty(document.documentElement, "scrollHeight", {
    configurable: true,
    value: 4096,
  });

  render(SessionConversation, { props: { messages } });
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(scrollTo).not.toHaveBeenCalled();
});

test("conversation does not scroll the document to the bottom when messages update near the bottom", async () => {
  const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 800 });
  Object.defineProperty(window, "scrollY", { configurable: true, value: 3200 });
  Object.defineProperty(document.documentElement, "scrollHeight", {
    configurable: true,
    value: 4096,
  });
  const { rerender } = render(SessionConversation, { props: { messages: [messages[0]] } });

  await new Promise((resolve) => setTimeout(resolve, 0));
  scrollTo.mockClear();
  await rerender({ messages });
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(scrollTo).not.toHaveBeenCalled();
});

test("conversation does not scroll the document to the bottom when refreshing while reading earlier messages", async () => {
  const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 800 });
  Object.defineProperty(window, "scrollY", { configurable: true, value: 600 });
  Object.defineProperty(document.documentElement, "scrollHeight", {
    configurable: true,
    value: 4096,
  });
  const { rerender } = render(SessionConversation, { props: { messages } });

  await new Promise((resolve) => setTimeout(resolve, 0));
  scrollTo.mockClear();
  await rerender({
    messages: [
      ...messages,
      { id: "message-3", role: "assistant", content: "New background update.", status: "sent" },
    ],
  });

  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(scrollTo).not.toHaveBeenCalled();
});

test("conversation names the current step of busy work without assistant text", () => {
  render(SessionConversation, {
    props: {
      sessionState: "busy",
      messages: [
        ...messages,
        {
          id: "message-3",
          turnId: "turn-live",
          role: "assistant",
          content: "",
          status: "pending",
          createdAt: "2026-06-11T00:00:00Z",
          thoughtSteps: [
            {
              id: "thought-1",
              kind: "tool_call",
              title: "bash",
              status: "started",
              content: "rg ThoughtSummary",
              occurredAt: null,
            },
            {
              id: "thought-2",
              kind: "tool_call",
              title: "read",
              status: "started",
              content: "ThoughtSummary.svelte",
              occurredAt: null,
            },
          ],
        },
      ],
    },
  });

  expect(screen.getAllByRole("timer", { name: "Turn running time" })).toHaveLength(1);
  expect(screen.getByRole("button", { name: "Show agent work steps" })).toHaveTextContent("read");
  expect(screen.queryByText("Working…")).not.toBeInTheDocument();
});
test("conversation keeps active Turn work collapsed when a queued user message follows it", () => {
  render(SessionConversation, {
    props: {
      sessionState: "busy",
      activeTurnId: "turn-live",
      messages: [
        ...messages,
        {
          id: "turn-live:working",
          turnId: "turn-live",
          role: "assistant",
          content: "",
          status: "pending",
          createdAt: "2026-06-11T00:00:00Z",
          thoughtSteps: [
            {
              id: "thinking-live",
              kind: "thinking",
              title: "Thinking",
              status: "started",
              content: "Inspecting the active turn.",
              occurredAt: null,
            },
          ],
        },
        {
          id: "queued:user",
          turnId: "turn-queued",
          role: "user",
          content: "Queued follow-up",
          status: "pending",
          createdAt: "2026-06-11T00:01:00Z",
        },
      ],
    },
  });

  const trigger = screen.getByRole("button", { name: "Show agent work steps" });
  expect(trigger).toHaveAttribute("aria-expanded", "false");
  const timer = screen.getByRole("timer", { name: "Turn running time" });
  expect(timer.closest("[data-chat-turn-id]")).toHaveAttribute("data-chat-turn-id", "turn-live");
  expect(screen.getByText("Inspecting the active turn.")).not.toBeVisible();
});
test("conversation shows agent working only once after an interrupted pending thought summary", () => {
  render(SessionConversation, {
    props: {
      sessionState: "busy",
      messages: [
        ...messages,
        {
          id: "interrupted-turn:working",
          turnId: "interrupted-turn",
          role: "assistant",
          content: "",
          status: "pending",
          createdAt: "2026-06-11T00:00:00Z",
          thoughtSteps: [
            {
              id: "thought-old",
              kind: "thinking",
              title: "Thinking",
              status: null,
              content: "Interrupted work",
              occurredAt: null,
            },
          ],
        },
        {
          id: "message-4",
          turnId: "next-turn",
          role: "user",
          content: "Try a smaller change.",
          status: "sent",
          createdAt: "2026-06-11T00:01:00Z",
        },
      ],
    },
  });

  expect(screen.getAllByRole("timer", { name: "Turn running time" })).toHaveLength(1);
  expect(screen.queryByRole("button", { name: /interrupt agent/i })).not.toBeInTheDocument();
  expect(screen.queryByText("Thought for 1 step")).not.toBeInTheDocument();
  expect(screen.getByText("Interrupted work")).not.toBeVisible();
});

test("conversation keeps non-trailing empty pending thought summaries idle while the session is busy", () => {
  render(SessionConversation, {
    props: {
      sessionState: "busy",
      messages: [
        ...messages,
        {
          id: "interrupted-turn:working",
          turnId: "interrupted-turn",
          role: "assistant",
          content: "",
          status: "pending",
          createdAt: "2026-06-11T00:00:00Z",
          thoughtSteps: [
            {
              id: "thought-old-1",
              kind: "tool_call",
              title: "read",
              status: "started",
              content: "Reading old file",
              occurredAt: null,
            },
            {
              id: "thought-old-2",
              kind: "tool_call",
              title: "bash",
              status: "started",
              content: "Running old command",
              occurredAt: null,
            },
          ],
        },
        {
          id: "message-4",
          turnId: "next-turn",
          role: "assistant",
          content: "Recovered with a final response.",
          status: "sent",
          createdAt: "2026-06-11T00:01:00Z",
        },
      ],
    },
  });

  expect(screen.getAllByRole("timer", { name: "Turn running time" })).toHaveLength(1);
  expect(screen.queryByLabelText("Thinking in progress")).not.toBeInTheDocument();
  expect(screen.queryByText("Thought for 2 steps")).not.toBeInTheDocument();
  expect(screen.getByText("Reading old file")).not.toBeVisible();
  expect(screen.getByText("Running old command")).not.toBeVisible();
});

test("conversation renders agent status without an assistant loading placeholder after the latest user message", async () => {
  render(SessionConversation, {
    props: {
      sessionState: "busy",
      messages: [
        ...messages,
        {
          id: "message-3",
          role: "user",
          content: "Keep going.",
          status: "sent",
        },
      ],
    },
  });

  expect(screen.getAllByRole("timer", { name: "Turn running time" })).toHaveLength(1);
  expect(
    document.querySelector('[data-chat-message-id="busy:assistant-loading-placeholder"]'),
  ).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /interrupt agent/i })).not.toBeInTheDocument();
});

test("conversation renders the starting status as a bottom status after the latest assistant content", () => {
  render(SessionConversation, {
    props: {
      sessionState: "starting",
      messages,
    },
  });

  const assistantMessage = document.querySelector('[data-chat-message-id="message-2"]');
  const response = screen.getByText("I will inspect it now.");
  const status = screen.getByLabelText("Session starting");
  const bottomStatus = status.closest("[data-chat-session-bottom-status]");

  expect(bottomStatus).toBeInTheDocument();
  expect(assistantMessage).not.toContainElement(status);
  expect(response.compareDocumentPosition(status) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

test("conversation renders assistant loading placeholder when session is starting and the latest user message has no assistant output", () => {
  render(SessionConversation, {
    props: {
      sessionState: "starting",
      messages: [
        ...messages,
        {
          id: "message-3",
          role: "user",
          content: "Now inspect the tests.",
          status: "sent",
        },
      ],
    },
  });

  expect(screen.getByText("Session starting")).toBeInTheDocument();
  expect(screen.getByLabelText("Session starting")).toHaveAttribute(
    "data-chat-session-bottom-status",
  );
  expect(screen.queryByLabelText("Agent status: Session starting")).not.toBeInTheDocument();
  expect(
    screen.queryByText("Waiting for the agent session to become ready."),
  ).not.toBeInTheDocument();
  expect(screen.queryByText("Working")).not.toBeInTheDocument();
  expect(screen.queryByText("No messages yet")).not.toBeInTheDocument();
});

test("conversation shows shared files on the Turn they are bound to", () => {
  const file = (file_id: string, turn_id: string | null) => ({
    file_id,
    turn_id,
    name: `${file_id}.txt`,
    path: `/tmp/${file_id}.txt`,
    size: 3,
    media_type: "text/plain",
    created_at: "2026-06-11T00:00:05Z",
  });
  render(SessionConversation, {
    props: {
      messages: messages.map((message) => ({ ...message, turnId: "turn-1" })),
      sessionId: "session-1",
      sharedFiles: [file("bound", "turn-1"), file("other", "turn-missing"), file("unbound", null)],
    },
  });
  const turn = document.querySelector('[data-chat-turn-id="turn-1"]');
  const list = screen.getByRole("list", { name: "Shared files in this turn" });
  expect(turn).toContainElement(list);
  expect(within(list).getByText("bound.txt")).toBeInTheDocument();
  expect(screen.queryByText("other.txt")).not.toBeInTheDocument();
  expect(screen.queryByText("unbound.txt")).not.toBeInTheDocument();
});
