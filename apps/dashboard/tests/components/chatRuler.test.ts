import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, test, vi } from 'vitest';
import ChatRuler from '../../src/lib/components/session-chat/ChatRuler.svelte';
import type { TurnView } from '../../src/api/types';

function turn(overrides: Partial<TurnView>): TurnView {
  return {
    turn_id: 'turn-root',
    session_id: 'session-1',
    parent_turn_id: null,
    topology_status: 'root',
    state: 'completed',
    input: { summary: 'Root question' },
    output: { summary: 'Root answer' },
    failure: null,
    created_at: '2026-05-14T00:00:00Z',
    started_at: '2026-05-14T00:00:01Z',
    completed_at: '2026-05-14T00:00:02Z',
    metadata: {},
    ...overrides,
  };
}

const turns = [
  turn({}),
  turn({
    turn_id: 'turn-current',
    parent_turn_id: 'turn-root',
    topology_status: 'linked',
    input: { summary: 'Current branch question' },
    output: { summary: 'Current branch answer' },
    created_at: '2026-05-14T00:01:00Z',
  }),
  turn({
    turn_id: 'turn-other',
    parent_turn_id: 'turn-root',
    topology_status: 'linked',
    input: { summary: 'Other branch question' },
    output: { summary: 'Other branch answer' },
    created_at: '2026-05-14T00:02:00Z',
  }),
];

describe('ChatRuler', () => {
  test('renders alternating user and assistant marks with hover summaries', async () => {
    const user = userEvent.setup();
    render(ChatRuler, {
      props: { turns, navigableTurnIds: turns.map((item) => item.turn_id) },
    });

    const marks = document.querySelectorAll('[data-chat-ruler-mark]');
    expect(marks).toHaveLength(6);
    const userMark = screen.getByRole('button', { name: 'User message: Root question' });
    expect(userMark).toHaveClass('h-px', 'w-6', 'bg-gray-300');
    expect(screen.getByRole('button', { name: 'Assistant message: Root answer' })).toHaveClass('h-px', 'w-4', 'bg-gray-300');

    await user.hover(userMark);
    expect(await screen.findByText('Root question')).toBeInTheDocument();
  });

  test('marks sibling branches only in tree mode', () => {
    const { rerender } = render(ChatRuler, {
      props: {
        turns,
        treeMode: false,
        navigableTurnIds: ['turn-root', 'turn-current'],
      },
    });
    expect(document.querySelectorAll('[data-chat-ruler-branch]')).toHaveLength(0);

    rerender({
      turns,
      treeMode: true,
      navigableTurnIds: ['turn-root', 'turn-current'],
    });
    expect(document.querySelectorAll('[data-chat-ruler-branch]')).toHaveLength(2);
  });

  test('navigates current-lineage marks and leaves other branches inactive', async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(ChatRuler, {
      props: {
        turns,
        treeMode: true,
        navigableTurnIds: ['turn-root', 'turn-current'],
        onNavigate,
      },
    });

    await user.click(screen.getByRole('button', { name: 'User message: Current branch question' }));
    expect(onNavigate).toHaveBeenCalledWith('turn-current', 'user');

    await user.click(screen.getByRole('button', {
      name: 'User message: Other branch question (not on the current branch)',
    }));
    expect(onNavigate).toHaveBeenCalledTimes(1);
  });
});
