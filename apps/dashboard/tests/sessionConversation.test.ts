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

test('conversation uses session busy state to keep thought summary active without showing Working text', () => {
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

  expect(screen.getByLabelText('Thinking in progress')).toBeInTheDocument();
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
  expect(screen.getByText('read')).toBeInTheDocument();
  expect(screen.getByText('Agent working')).toBeInTheDocument();
  expect(screen.queryByText('Working…')).not.toBeInTheDocument();
});

test('conversation shows agent working only on the latest pending assistant placeholder after an interrupt', () => {
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
  expect(screen.getAllByRole('button', { name: /interrupt agent/i })).toHaveLength(1);
  expect(screen.getByText('Thought for 1 step')).toBeInTheDocument();
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

  expect(screen.queryByText('Agent working')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
  expect(screen.getByText('Thought for 2 steps')).toBeInTheDocument();
  expect(screen.queryByText('Reading old file')).not.toBeInTheDocument();
  expect(screen.queryByText('Running old command')).not.toBeInTheDocument();
});

test('conversation renders an interrupt button on the agent working placeholder when enabled', async () => {
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

  const interruptButton = screen.getByRole('button', { name: /interrupt agent/i });
  expect(interruptButton).toBeInTheDocument();
  expect(interruptButton).toHaveAttribute('title', 'Interrupt agent');
  expect(interruptButton.textContent?.trim()).toBe('');
  expect(interruptButton.querySelector('svg.lucide-square')).toBeInTheDocument();
  expect(interruptButton).not.toHaveClass('border-border');

  await fireEvent.click(interruptButton);

  expect(onInterrupt).toHaveBeenCalledTimes(1);
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
  expect(screen.getByText('Waiting for the agent session to become ready.')).toBeInTheDocument();
  expect(screen.queryByTestId('blocks-wave-spinner')).not.toBeInTheDocument();
  expect(screen.queryByText('Working')).not.toBeInTheDocument();
  expect(screen.queryByText('No messages yet')).not.toBeInTheDocument();
});
