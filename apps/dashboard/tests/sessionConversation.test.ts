import { render, screen, waitFor } from '@testing-library/svelte';
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

test('conversation scrolls the document to the bottom when a new message arrives', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  const { rerender } = render(SessionConversation, { props: { messages: [messages[0]] } });

  await waitFor(() => expect(scrollTo).toHaveBeenCalled());
  scrollTo.mockClear();
  await rerender({ messages });

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
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
  expect(screen.getByText('bash')).toBeInTheDocument();
  expect(screen.getByText('read')).toBeInTheDocument();
  expect(screen.getByText('Agent working')).toBeInTheDocument();
  expect(screen.queryByText('Working…')).not.toBeInTheDocument();
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
