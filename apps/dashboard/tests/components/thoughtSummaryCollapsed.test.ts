import { render, screen } from '@testing-library/svelte';
import { expect, test, vi } from 'vitest';
import ThoughtSummaryCollapsed from '../../src/lib/components/session-chat/ThoughtSummaryCollapsed.svelte';

test('collapsed thought summary fills container without icon and uses smaller title text', () => {
  const { container } = render(ThoughtSummaryCollapsed, {
    props: {
      step: {
        id: 'thought-1',
        kind: 'thinking',
        title: 'Thinking',
        status: null,
        content: 'Inspecting the project.',
        occurredAt: '2026-06-11T00:00:00Z',
      },
      stepCount: 1,
      active: false,
      onOpen: vi.fn(),
    },
  });

  const button = screen.getByRole('button', { name: 'View thought details' });
  expect(button).toHaveClass('w-full');
  expect(container.querySelector('svg')).not.toBeInTheDocument();
  expect(screen.getByText('Thinking')).toHaveClass('text-sm');
  expect(screen.getByText('Thinking')).not.toHaveClass('text-base');
});
