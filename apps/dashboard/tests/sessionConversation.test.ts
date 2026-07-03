import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, test, vi } from 'vitest';
import SessionConversation from '../src/lib/components/session-chat/SessionConversation.svelte';
import type { SessionChatMessage } from '../src/lib/session-chat/sessionChat';

const testDir = dirname(fileURLToPath(import.meta.url));
const sessionConversationSourcePath = resolve(testDir, '../src/lib/components/session-chat/SessionConversation.svelte');

class TestIntersectionObserver implements IntersectionObserver {
  static instances: TestIntersectionObserver[] = [];

  readonly root: Element | Document | null = null;
  readonly rootMargin = '0px';
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

  takeRecords(): IntersectionObserverEntry[] { return []; }

  trigger(isIntersecting: boolean): void {
    if (!this.observedElement) return;
    this.callback([{ isIntersecting, target: this.observedElement } as IntersectionObserverEntry], this);
  }
}

function installIntersectionObserverMock(): void {
  TestIntersectionObserver.instances = [];
  Object.defineProperty(window, 'IntersectionObserver', { configurable: true, writable: true, value: TestIntersectionObserver });
  Object.defineProperty(globalThis, 'IntersectionObserver', { configurable: true, writable: true, value: TestIntersectionObserver });
}

const messages: SessionChatMessage[] = [
  {
    id: 'message-1',
    role: 'user',
    content: 'Please inspect the repo.',
    status: 'sent',
  },
  {
    id: 'message-2',
    role: 'assistant',
    content: 'I will inspect it now.',
    status: 'sent',
  },
];

test('conversation renders messages without role headers', () => {
  render(SessionConversation, { props: { messages } });

  expect(screen.getByText('Please inspect the repo.')).toBeInTheDocument();
  expect(screen.getByText('I will inspect it now.')).toBeInTheDocument();
  expect(screen.queryByText('You')).not.toBeInTheDocument();
  expect(screen.queryByText('AI')).not.toBeInTheDocument();
});

test('conversation groups assistant-side items after each user message', () => {
  render(SessionConversation, {
    props: {
      sessionState: 'busy',
      messages: [
        ...messages,
        {
          id: 'message-3',
          role: 'user',
          content: 'Keep going.',
          status: 'sent',
        },
      ],
    },
  });

  const assistantGroups = document.querySelectorAll('[data-chat-assistant-group]');
  expect(assistantGroups).toHaveLength(2);
  expect(assistantGroups[0]).toContainElement(document.querySelector('[data-chat-message-id="message-2"]'));
  expect(assistantGroups[1]).toContainElement(document.querySelector('[data-chat-agent-status]'));
  expect(assistantGroups[1]).toHaveClass('chat-turn-tail-space');
  expect(document.querySelector('[data-chat-tail-spacer]')).not.toBeInTheDocument();
});

test('conversation expands the latest assistant group instead of the assistant message', () => {
  render(SessionConversation, { props: { messages } });

  const assistantGroup = document.querySelector('[data-chat-assistant-group]');
  expect(assistantGroup).toHaveClass('chat-turn-tail-space');
  expect(assistantGroup).toContainElement(document.querySelector('[data-chat-message-id="message-2"]'));
  expect(document.querySelector('[data-chat-message-id="message-2"]')).not.toHaveClass('chat-turn-tail-space');
  expect(document.querySelector('[data-chat-tail-spacer]')).not.toBeInTheDocument();
});

test('conversation tail space does not force a fixed minimum floor', () => {
  const source = readFileSync(sessionConversationSourcePath, 'utf8');

  expect(source).toContain('min-height: calc(100dvh - 31rem);');
  expect(source).not.toContain('min-height: max(8rem, calc(100dvh - 31rem));');
});

test('conversation constrains assistant code blocks to the message width', async () => {
  const longLine = 'const value = "' + 'x'.repeat(240) + '";';
  render(SessionConversation, {
    props: {
      messages: [
        messages[0],
        {
          id: 'message-code',
          role: 'assistant',
          content: `Here is the code:\n\n\`\`\`ts\n${longLine}\n\`\`\``,
          status: 'sent',
        },
      ],
    },
  });

  expect(await screen.findByRole('button', { name: /copy code block/i })).toBeInTheDocument();
  expect(document.querySelector('[data-chat-conversation-content]')).toHaveClass('min-w-0');
  expect(document.querySelector('[data-role="assistant"]')).toHaveClass('min-w-0');
  expect(document.querySelector('pre')).toHaveClass('max-w-full');
  expect(document.querySelector('[data-code-block-body]')).toHaveClass('max-w-full', 'overflow-x-auto');
});

test('conversation shows the current agent status above only the latest assistant reply', () => {
  render(SessionConversation, {
    props: {
      sessionState: 'busy',
      messages: [
        ...messages,
        {
          id: 'message-3',
          role: 'user',
          content: 'Check one more file.',
          status: 'sent',
        },
        {
          id: 'message-4',
          role: 'assistant',
          content: 'I checked it.',
          status: 'sent',
        },
      ],
    },
  });

  expect(screen.getAllByText('Agent working')).toHaveLength(1);
  const latestAssistantMessage = screen.getByText('I checked it.').closest('[data-chat-message-id]');
  expect(latestAssistantMessage).toContainElement(screen.getByText('Agent working'));
  expect(screen.queryByText('Agent working')?.compareDocumentPosition(screen.getByText('I checked it.')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getByLabelText('Agent status: Agent working')).toBeInTheDocument();
});

test('conversation hides the agent status component while the session is idle', () => {
  render(SessionConversation, { props: { sessionState: 'idle', messages } });

  expect(screen.queryByLabelText(/agent status/i)).not.toBeInTheDocument();
  expect(screen.queryByText('Agent idle')).not.toBeInTheDocument();
});

test('conversation shows an interrupt button in the busy agent status', async () => {
  const onInterrupt = vi.fn();

  render(SessionConversation, {
    props: {
      sessionState: 'busy',
      messages,
      interruptEnabled: true,
      onInterrupt,
    },
  });

  const interruptButton = screen.getByRole('button', { name: /interrupt agent/i });
  expect(interruptButton).toHaveAttribute('title', 'Interrupt agent');
  expect(interruptButton).toHaveTextContent('Interrupt');
  await fireEvent.click(interruptButton);

  expect(onInterrupt).toHaveBeenCalledTimes(1);
});

test('conversation renders exited status as a left-aligned bottom status after the conversation', () => {
  render(SessionConversation, { props: { sessionState: 'exited', messages } });

  expect(screen.queryByLabelText(/agent status/i)).not.toBeInTheDocument();
  expect(screen.queryByText('Session exited')).not.toBeInTheDocument();

  const bottomStatus = screen.getByText('session exited · send a message to resume');
  const bottomStatusContainer = bottomStatus.closest('[data-chat-session-bottom-status]');
  expect(bottomStatusContainer).toBeInTheDocument();
  expect(bottomStatusContainer).toHaveClass('justify-start');
  expect(bottomStatusContainer?.querySelector('.h-px')).not.toBeInTheDocument();
  expect(screen.getByText('I will inspect it now.').compareDocumentPosition(bottomStatus) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

test('conversation renders interrupted status as a left-aligned bottom status after the conversation', () => {
  render(SessionConversation, { props: { sessionState: 'interrupted', messages } });

  expect(screen.queryByLabelText(/agent status/i)).not.toBeInTheDocument();

  const bottomStatus = screen.getByText('session interrupted');
  const bottomStatusContainer = bottomStatus.closest('[data-chat-session-bottom-status]');
  expect(bottomStatusContainer).toBeInTheDocument();
  expect(bottomStatusContainer).toHaveClass('justify-start');
  expect(bottomStatusContainer?.querySelector('.h-px')).not.toBeInTheDocument();
  expect(screen.getByText('I will inspect it now.').compareDocumentPosition(bottomStatus) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

test('conversation copies assistant reply content with the http-compatible fallback', async () => {
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined });
  document.execCommand = vi.fn().mockReturnValue(true);
  const execCommand = vi.mocked(document.execCommand);

  render(SessionConversation, { props: { messages } });

  expect(screen.queryByRole('button', { name: /copy user message/i })).not.toBeInTheDocument();
  const copyButton = screen.getByRole('button', { name: /copy assistant reply/i });
  expect(copyButton.parentElement).toHaveClass('justify-start');
  expect(copyButton.parentElement).not.toHaveClass('justify-end');
  await fireEvent.click(copyButton);

  expect(execCommand).toHaveBeenCalledWith('copy');
  expect(screen.getByRole('button', { name: /assistant reply copied/i })).toBeInTheDocument();
});

test('conversation waits to observe earlier history until history observer is enabled', async () => {
  installIntersectionObserverMock();
  const onLoadMoreHistory = vi.fn();

  const { rerender } = render(SessionConversation, {
    props: { messages, hasMoreHistory: true, historyObserverEnabled: false, onLoadMoreHistory },
  });

  expect(TestIntersectionObserver.instances).toHaveLength(0);
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();

  await rerender({ messages, hasMoreHistory: true, historyObserverEnabled: true, onLoadMoreHistory });

  await waitFor(() => expect(TestIntersectionObserver.instances.length).toBeGreaterThan(0));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();
});

test('conversation waits for an extra upward pull after the top sentinel intersects before loading history', async () => {
  installIntersectionObserverMock();
  const onLoadMoreHistory = vi.fn();
  render(SessionConversation, {
    props: { messages, hasMoreHistory: true, historyObserverEnabled: true, onLoadMoreHistory },
  });

  expect(screen.queryByRole('button', { name: /load earlier messages/i })).not.toBeInTheDocument();
  await waitFor(() => expect(TestIntersectionObserver.instances.length).toBeGreaterThan(0));
  TestIntersectionObserver.instances.at(-1)?.trigger(false);
  window.dispatchEvent(new WheelEvent('wheel', { deltaY: -160 }));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();

  TestIntersectionObserver.instances.at(-1)?.trigger(true);
  expect(await screen.findByText('Keep scrolling up to load earlier messages')).toBeInTheDocument();
  expect(onLoadMoreHistory).not.toHaveBeenCalled();

  window.dispatchEvent(new WheelEvent('wheel', { deltaY: -64 }));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();
  window.dispatchEvent(new WheelEvent('wheel', { deltaY: -64 }));

  await waitFor(() => expect(onLoadMoreHistory).toHaveBeenCalledTimes(1));
});

test('conversation preserves the visible history anchor after prepending earlier messages', async () => {
  installIntersectionObserverMock();
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const requestAnimationFrame = vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
    callback(0);
    return 1;
  });
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 120 });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 1000 });
  const onLoadMoreHistory = vi.fn(async () => {
    Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 1400 });
  });

  render(SessionConversation, {
    props: { messages, hasMoreHistory: true, historyObserverEnabled: true, onLoadMoreHistory },
  });

  await waitFor(() => expect(TestIntersectionObserver.instances.length).toBeGreaterThan(0));
  TestIntersectionObserver.instances.at(-1)?.trigger(true);
  window.dispatchEvent(new WheelEvent('wheel', { deltaY: -120 }));

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 520 }));
  expect(onLoadMoreHistory).toHaveBeenCalledTimes(1);
  scrollTo.mockRestore();
  requestAnimationFrame.mockRestore();
});

test('conversation does not scroll the document to the bottom on initial render', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 800 });
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 3200 });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });

  render(SessionConversation, { props: { messages } });
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(scrollTo).not.toHaveBeenCalled();
});

test('conversation does not scroll the document to the bottom when messages update near the bottom', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 800 });
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 3200 });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  const { rerender } = render(SessionConversation, { props: { messages: [messages[0]] } });

  await new Promise((resolve) => setTimeout(resolve, 0));
  scrollTo.mockClear();
  await rerender({ messages });
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(scrollTo).not.toHaveBeenCalled();
});

test('conversation does not scroll the document to the bottom when refreshing while reading earlier messages', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 800 });
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 600 });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  const { rerender } = render(SessionConversation, { props: { messages } });

  await new Promise((resolve) => setTimeout(resolve, 0));
  scrollTo.mockClear();
  await rerender({ messages: [...messages, { id: 'message-3', role: 'assistant', content: 'New background update.', status: 'sent' }] });

  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(scrollTo).not.toHaveBeenCalled();
});

test('conversation shows agent status for a busy pending assistant message with thought steps', () => {
  render(SessionConversation, {
    props: {
      sessionState: 'busy',
      messages: [
        ...messages,
        {
          id: 'message-3',
          turnId: 'turn-live',
          role: 'assistant',
          content: '',
          status: 'pending',
          createdAt: '2026-06-11T00:00:00Z',
          thoughtSteps: [
            { id: 'thought-1', kind: 'tool_call', title: 'bash', status: 'started', content: 'rg ThoughtSummary', occurredAt: null },
            { id: 'thought-2', kind: 'tool_call', title: 'read', status: 'started', content: 'ThoughtSummary.svelte', occurredAt: null },
          ],
        },
      ],
    },
  });

  expect(screen.getByLabelText('Agent status: Agent working')).toBeInTheDocument();
  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
  expect(screen.getByText('read')).toBeInTheDocument();
  expect(screen.queryByText('Waiting for the agent to report its next output.')).not.toBeInTheDocument();
  expect(screen.queryByText('Working…')).not.toBeInTheDocument();
});

test('conversation shows agent working only once after an interrupted pending thought summary', () => {
  render(SessionConversation, {
    props: {
      sessionState: 'busy',
      messages: [
        ...messages,
        {
          id: 'interrupted-turn:working',
          turnId: 'interrupted-turn',
          role: 'assistant',
          content: '',
          status: 'pending',
          createdAt: '2026-06-11T00:00:00Z',
          thoughtSteps: [
            { id: 'thought-old', kind: 'thinking', title: 'Thinking', status: null, content: 'Interrupted work', occurredAt: null },
          ],
        },
        {
          id: 'message-4',
          turnId: 'next-turn',
          role: 'user',
          content: 'Try a smaller change.',
          status: 'sent',
          createdAt: '2026-06-11T00:01:00Z',
        },
      ],
      interruptEnabled: true,
    },
  });

  expect(screen.getAllByText('Agent working')).toHaveLength(1);
  expect(screen.queryByRole('button', { name: /interrupt agent/i })).not.toBeInTheDocument();
  expect(screen.queryByText('Thought for 1 step')).not.toBeInTheDocument();
  expect(screen.queryByText('Interrupted work')).not.toBeInTheDocument();
});

test('conversation keeps non-trailing empty pending thought summaries idle while the session is busy', () => {
  render(SessionConversation, {
    props: {
      sessionState: 'busy',
      messages: [
        ...messages,
        {
          id: 'interrupted-turn:working',
          turnId: 'interrupted-turn',
          role: 'assistant',
          content: '',
          status: 'pending',
          createdAt: '2026-06-11T00:00:00Z',
          thoughtSteps: [
            { id: 'thought-old-1', kind: 'tool_call', title: 'read', status: 'started', content: 'Reading old file', occurredAt: null },
            { id: 'thought-old-2', kind: 'tool_call', title: 'bash', status: 'started', content: 'Running old command', occurredAt: null },
          ],
        },
        {
          id: 'message-4',
          turnId: 'next-turn',
          role: 'assistant',
          content: 'Recovered with a final response.',
          status: 'sent',
          createdAt: '2026-06-11T00:01:00Z',
        },
      ],
      interruptEnabled: true,
    },
  });

  expect(screen.getByText('Agent working')).toBeInTheDocument();
  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
  expect(screen.queryByText('Thought for 2 steps')).not.toBeInTheDocument();
  expect(screen.queryByText('Reading old file')).not.toBeInTheDocument();
  expect(screen.queryByText('Running old command')).not.toBeInTheDocument();
});

test('conversation renders agent status without an assistant loading placeholder after the latest user message', async () => {
  const onInterrupt = vi.fn();

  render(SessionConversation, {
    props: {
      sessionState: 'busy',
      messages: [
        ...messages,
        {
          id: 'message-3',
          role: 'user',
          content: 'Keep going.',
          status: 'sent',
        },
      ],
      interruptEnabled: true,
      onInterrupt,
    },
  });

  expect(screen.getByLabelText('Agent status: Agent working')).toBeInTheDocument();
  expect(document.querySelector('[data-chat-message-id="busy:assistant-loading-placeholder"]')).not.toBeInTheDocument();
  expect(document.querySelector('[data-chat-agent-status]')).toHaveClass('is-assistant', 'items-start');
  expect(screen.getByRole('button', { name: /interrupt agent/i })).toBeInTheDocument();

  expect(onInterrupt).not.toHaveBeenCalled();
});

test('conversation renders assistant loading placeholder when session is starting and the latest user message has no assistant output', () => {
  render(SessionConversation, {
    props: {
      sessionState: 'starting',
      messages: [
        ...messages,
        {
          id: 'message-3',
          role: 'user',
          content: 'Now inspect the tests.',
          status: 'sent',
        },
      ],
    },
  });

  expect(screen.getByText('Session starting')).toBeInTheDocument();
  expect(screen.getByLabelText('Agent status: Session starting')).toBeInTheDocument();
  expect(screen.queryByText('Waiting for the agent session to become ready.')).not.toBeInTheDocument();
  expect(screen.getByTestId('blocks-wave-spinner')).toBeInTheDocument();
  expect(screen.queryByText('Working')).not.toBeInTheDocument();
  expect(screen.queryByText('No messages yet')).not.toBeInTheDocument();
});

test('conversation lazy-loads the draft DAG renderer when the draft sheet is opened', async () => {
  const loadDraftDagFlow = vi.fn(() => new Promise<never>(() => {}));

  render(SessionConversation, {
    props: {
      messages,
      plannerTaskId: 'task-1',
      draftPlannerProposal: {
        proposal_id: 'proposal-1',
        task_id: 'task-1',
        mode: 'draft',
        state: 'pending_review',
        summary: 'Draft plan summary.',
        proposal_json: {
          work_items: [{ id: 'item-1', title: 'Inspect repo' }],
          edges: [],
        },
        validation_json: {},
        created_by_session_id: 'session-1',
        revision: 1,
        supersedes_proposal_id: null,
        created_at: '2026-06-11T00:00:00Z',
        updated_at: '2026-06-11T00:00:00Z',
      },
      loadDraftDagFlow,
    },
  });

  expect(loadDraftDagFlow).not.toHaveBeenCalled();

  await fireEvent.click(screen.getByRole('button', { name: /view draft dag/i }));

  expect(loadDraftDagFlow).toHaveBeenCalledTimes(1);
  expect(await screen.findByText('Loading DAG renderer…')).toBeInTheDocument();
});
