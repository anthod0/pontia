import { render, screen } from '@testing-library/svelte';
import { expect, test, vi } from 'vitest';
import ThoughtSummaryCollapsed from '../../src/lib/components/session-chat/ThoughtSummaryCollapsed.svelte';
import type { SessionChatThoughtStep } from '../../src/lib/session-chat/sessionChat';

function step(overrides: Partial<SessionChatThoughtStep>): SessionChatThoughtStep {
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

test('collapsed thought summary shows only the latest tool call row without chrome and uses compact text', () => {
  const { container } = render(ThoughtSummaryCollapsed, {
    props: {
      steps: [
        step({ id: 'thought-1', title: 'Ignored earlier step', content: 'Old content' }),
        step({ id: 'thought-2', kind: 'tool_call', title: 'Ignored previous call', content: 'pnpm --dir apps/dashboard run check' }),
        step({ id: 'thought-3', kind: 'tool_call', title: 'read', content: 'apps/dashboard/src/lib/components/session-chat/ThoughtSummaryCollapsed.svelte' }),
      ],
      active: false,
      onOpen: vi.fn(),
    },
  });

  const button = screen.getByRole('button', { name: 'View thought details' });
  expect(button).toHaveClass('w-full');
  expect(button).not.toHaveClass('bg-muted/20');
  expect(button).not.toHaveClass('hover:bg-muted/35');
  expect(button).not.toHaveClass('border');
  expect(button).not.toHaveClass('border-border/70');
  expect(button).not.toHaveClass('hover:border-border');
  expect(button).not.toHaveClass('shadow-sm');
  expect(screen.queryByText('3 steps')).not.toBeInTheDocument();
  expect(screen.queryByText('completed')).not.toBeInTheDocument();
  expect(screen.getAllByLabelText('completed')).toHaveLength(1);
  expect(container.querySelector('svg')).toBeInTheDocument();
  expect(screen.queryByText('Ignored earlier step')).not.toBeInTheDocument();
  expect(screen.queryByText('Ignored previous call')).not.toBeInTheDocument();
  expect(screen.queryByText('pnpm --dir apps/dashboard run check')).not.toBeInTheDocument();
  expect(screen.getByText('read')).toHaveClass('text-sm');
  expect(screen.getByText('read')).not.toHaveClass('text-base');
  expect(screen.getByText('apps/dashboard/src/lib/components/session-chat/ThoughtSummaryCollapsed.svelte')).toHaveClass('line-clamp-1');
  expect(screen.getByText('apps/dashboard/src/lib/components/session-chat/ThoughtSummaryCollapsed.svelte')).not.toHaveClass('line-clamp-2');
});

test('collapsed thought summary marks refreshed rows with an upward rolling animation', async () => {
  const { rerender } = render(ThoughtSummaryCollapsed, {
    props: {
      steps: [step({ id: 'thought-1', kind: 'tool_call', title: 'bash' }), step({ id: 'thought-2', kind: 'tool_call', title: 'read' })],
      active: false,
      onOpen: vi.fn(),
    },
  });

  await rerender({
    steps: [
      step({ id: 'thought-1', kind: 'tool_call', title: 'bash' }),
      step({ id: 'thought-2', kind: 'tool_call', title: 'read' }),
      step({ id: 'thought-3', kind: 'tool_call', title: 'edit' }),
    ],
    active: false,
    onOpen: vi.fn(),
  });

  expect(screen.getByTestId('thought-summary-rolling-stack')).toHaveClass('thought-summary-roll-up');
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
  expect(screen.getByText('read')).toBeInTheDocument();
  expect(screen.getByText('edit')).toBeInTheDocument();
});
