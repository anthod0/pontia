import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, test, vi } from 'vitest';
import SessionConversation from '../src/lib/components/session-chat/SessionConversation.svelte';
import type { SessionChatMessage } from '../src/lib/session-chat/sessionChat';

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

test('conversation renders exited status as a terminal divider after the conversation', () => {
  render(SessionConversation, { props: { sessionState: 'exited', messages } });

  expect(screen.queryByLabelText(/agent status/i)).not.toBeInTheDocument();
  expect(screen.queryByText('Session exited')).not.toBeInTheDocument();

  const terminalStatus = screen.getByText('session exited · send a message to resume');
  expect(terminalStatus).toBeInTheDocument();
  expect(terminalStatus.closest('[data-chat-session-terminal-status]')).toBeInTheDocument();
  expect(screen.getByText('I will inspect it now.').compareDocumentPosition(terminalStatus) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
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

test('conversation loads earlier history when scrolled to the top', async () => {
  const onLoadMoreHistory = vi.fn();
  render(SessionConversation, { props: { messages, hasMoreHistory: true, onLoadMoreHistory } });

  expect(screen.queryByRole('button', { name: /load earlier messages/i })).not.toBeInTheDocument();

  Object.defineProperty(window, 'scrollY', { configurable: true, value: 120 });
  window.dispatchEvent(new Event('scroll'));
  expect(onLoadMoreHistory).not.toHaveBeenCalled();

  Object.defineProperty(window, 'scrollY', { configurable: true, value: 40 });
  window.dispatchEvent(new Event('scroll'));

  await waitFor(() => expect(onLoadMoreHistory).toHaveBeenCalledTimes(1));
});

test('conversation auto-loads earlier history when initial content is already at the top', async () => {
  const onLoadMoreHistory = vi.fn();
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 0 });

  render(SessionConversation, { props: { messages, hasMoreHistory: true, onLoadMoreHistory } });

  await waitFor(() => expect(onLoadMoreHistory).toHaveBeenCalledTimes(1));
});

test('conversation keeps the first visible message anchored after prepending history', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 40 });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 1000 });
  let firstMessageTop = 100;
  const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    if (this.dataset.chatMessageId === 'message-1') {
      return { top: firstMessageTop, bottom: firstMessageTop + 40, left: 0, right: 100, width: 100, height: 40, x: 0, y: firstMessageTop, toJSON: () => ({}) };
    }
    return { top: -100, bottom: -60, left: 0, right: 100, width: 100, height: 40, x: 0, y: -100, toJSON: () => ({}) };
  });
  const onLoadMoreHistory = vi.fn(async () => {
    firstMessageTop = 132;
  });

  render(SessionConversation, { props: { messages, hasMoreHistory: true, onLoadMoreHistory } });

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 72 }));
  rectSpy.mockRestore();
  scrollTo.mockRestore();
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

test('conversation scrolls the document to the bottom when a new message arrives while already near the bottom', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 800 });
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 3200 });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  const { rerender } = render(SessionConversation, { props: { messages: [messages[0]] } });

  await new Promise((resolve) => setTimeout(resolve, 0));
  scrollTo.mockClear();
  await rerender({ messages });

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
});

test('conversation can use an external auto-scroll key instead of message content changes', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 800 });
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 3200 });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  const { rerender } = render(SessionConversation, { props: { messages: [messages[0]], autoScrollKey: 'cursor-1' } });

  await new Promise((resolve) => setTimeout(resolve, 0));
  scrollTo.mockClear();
  await rerender({ messages: [{ ...messages[0], content: 'Same timeline cursor, rebuilt message.' }], autoScrollKey: 'cursor-1' });
  await new Promise((resolve) => setTimeout(resolve, 0));

  expect(scrollTo).not.toHaveBeenCalled();

  await rerender({ messages, autoScrollKey: 'cursor-2' });
  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
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
