import { render, screen } from '@testing-library/svelte';
import { expect, test, vi } from 'vitest';
import MessageComposer from '../../src/components/chat/MessageComposer.svelte';

function renderComposer() {
  return render(MessageComposer, {
    props: {
      value: '',
      fullscreen: true,
      onSubmit: vi.fn(),
    },
  });
}

test('message composer textarea grows with content using field sizing', () => {
  renderComposer();

  const textarea = screen.getByPlaceholderText('Send a follow-up message…');

  expect(textarea).toHaveClass('field-sizing-content');
  expect(textarea).toHaveClass('max-h-48');
  expect(textarea).toHaveClass('overflow-y-auto');
  expect(textarea).not.toHaveClass('h-10');
});

test('message composer keeps resize control mobile-only', () => {
  renderComposer();

  expect(screen.queryByRole('button', { name: 'Collapse message composer' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Expand message composer' })).toHaveClass('sm:hidden');
});
