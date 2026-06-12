import { render, screen } from '@testing-library/svelte';
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
