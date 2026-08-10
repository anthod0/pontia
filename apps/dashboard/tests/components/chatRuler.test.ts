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
    expect(userMark).toHaveClass('h-full', 'w-full');
    expect(userMark.parentElement).toHaveClass('h-2');
    expect(userMark.querySelector('[data-chat-ruler-line]')).toHaveClass('h-px', 'w-[10px]', 'bg-gray-300');
    expect(
      screen
        .getByRole('button', { name: 'Assistant message: Root answer' })
        .querySelector('[data-chat-ruler-line]'),
    ).toHaveClass('h-px', 'w-[5px]', 'bg-gray-300');

    await user.hover(userMark);
    const summary = await screen.findByText('Root question');
    const tooltipLayout = summary.parentElement;
    expect(summary).toHaveClass('text-gray-600');
    expect(tooltipLayout).toHaveClass('flex-col', 'items-start');
    expect(tooltipLayout?.parentElement).toHaveClass(
      'border',
      'border-gray-200',
      'bg-gray-100',
      'text-gray-900',
      'shadow-md',
    );
    expect(tooltipLayout?.parentElement?.querySelector('.hidden')).toBeInTheDocument();
    expect(tooltipLayout?.children[0]).toHaveTextContent('User');
    expect(tooltipLayout?.children[1]).toHaveTextContent('Root question');
  });

  test('renders sibling branches as darker user lines only in tree mode', () => {
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
    const branchLines = document.querySelectorAll('[data-chat-ruler-branch]');
    expect(branchLines).toHaveLength(1);
    expect(branchLines[0].tagName).toBe('SPAN');
    expect(branchLines[0]).toHaveClass('h-px', 'w-[10px]', 'bg-gray-500');
    expect(branchLines[0].closest('button')).toHaveAttribute('data-turn-id', 'turn-current');
    expect(screen.queryByRole('button', { name: 'User message: Other branch question' })).not.toBeInTheDocument();
    expect(document.querySelector('[data-chat-ruler] svg')).not.toBeInTheDocument();
  });

  test('renders and navigates only current-lineage marks', async () => {
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

    expect(document.querySelectorAll('[data-chat-ruler-mark]')).toHaveLength(4);
    expect(screen.queryByRole('button', { name: 'User message: Other branch question' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'User message: Current branch question' }));
    expect(onNavigate).toHaveBeenCalledTimes(1);
    expect(onNavigate).toHaveBeenCalledWith('turn-current', 'user');
  });
});
