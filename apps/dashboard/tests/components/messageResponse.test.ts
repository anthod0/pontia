import { render, screen, waitFor, within } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import MessageResponse from '../../src/lib/components/ai-elements/message/message-response.svelte';

test('markdown links open in a new tab and show an external link icon', async () => {
  render(MessageResponse, {
    props: {
      content: 'Read the [docs](https://example.com/docs).',
      markdown: true,
    },
  });

  const link = screen.getByRole('link', { name: /docs/i });

  await waitFor(() => {
    expect(link).toHaveAttribute('target', '_blank');
  });
  expect(link).toHaveAttribute('rel', 'noopener noreferrer');
  expect(within(link).getByTestId('markdown-external-link-icon')).toHaveAttribute('aria-hidden', 'true');
});
