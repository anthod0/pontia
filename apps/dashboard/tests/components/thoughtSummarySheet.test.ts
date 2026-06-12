import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expect, test } from 'vitest';
import ThoughtSummarySheet from '../../src/lib/components/session-chat/ThoughtSummarySheet.svelte';
import type { SessionChatThoughtStep } from '../../src/lib/session-chat/sessionChat';

function step(overrides: Partial<SessionChatThoughtStep> = {}): SessionChatThoughtStep {
  return {
    id: 'thought-1',
    kind: 'thinking',
    title: 'Thinking',
    status: 'completed',
    content: 'Inspecting the project.',
    occurredAt: '2026-06-11T00:00:00Z',
    ...overrides,
  };
}

test('thought details sheet overrides the right-side sheet desktop max width for long tool output', async () => {
  render(ThoughtSummarySheet, {
    props: {
      steps: [step()],
      open: true,
    },
  });

  const dialog = await screen.findByRole('dialog');
  expect(dialog).toHaveAttribute('data-side', 'right');
  expect(dialog).toHaveClass('data-[side=right]:sm:max-w-4xl');
});

test('thought details defaults to newest first and can switch to oldest first', async () => {
  const user = userEvent.setup();
  render(ThoughtSummarySheet, {
    props: {
      steps: [
        step({ id: 'thought-1', content: 'First step' }),
        step({ id: 'thought-2', content: 'Second step' }),
        step({ id: 'thought-3', content: 'Third step' }),
      ],
      open: true,
    },
  });

  const third = await screen.findByText('Third step');
  const first = screen.getByText('First step');
  expect(third.compareDocumentPosition(first) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

  await user.click(screen.getByRole('button', { name: /show oldest first/i }));

  expect(first.compareDocumentPosition(third) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getByRole('button', { name: /show newest first/i })).toBeInTheDocument();
});
