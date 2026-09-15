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

test('does not create executable elements from raw HTML', () => {
  const { container } = render(MessageResponse, {
    props: {
      content: '<script>alert("unsafe")</script>',
      markdown: true,
    },
  });

  expect(container.querySelector('script')).not.toBeInTheDocument();
  expect(container).toHaveTextContent('alert("unsafe")');
});

test('keeps an unclosed streaming fence as one code block until it closes', async () => {
  const initialContent = 'Stable paragraph.\n\n```ts\nconst first = 1;\n';
  const { container, rerender } = render(MessageResponse, {
    props: {
      content: initialContent,
      markdown: true,
      streaming: true,
      streamId: 'assistant-message-1',
    },
  });

  expect(await screen.findByText('Stable paragraph.')).toBeInTheDocument();
  await waitFor(() => expect(container.querySelector('[data-code-block] code')).toHaveTextContent('const first = 1;'));
  const stableParagraph = screen.getByText('Stable paragraph.');
  expect(container.querySelectorAll('[data-code-block]')).toHaveLength(1);

  const contentWithBlankLine = `${initialContent}\nconst second = 2;\n`;
  await rerender({
    content: contentWithBlankLine,
    markdown: true,
    streaming: true,
    streamId: 'assistant-message-1',
  });

  await waitFor(() => expect(container.querySelector('[data-code-block] code')).toHaveTextContent('const second = 2;'));
  expect(screen.getByText('Stable paragraph.')).toBe(stableParagraph);
  expect(container.querySelectorAll('[data-code-block]')).toHaveLength(1);
  const codeLines = container.querySelectorAll('[data-code-block] .line');
  expect(codeLines).toHaveLength(3);
  expect(codeLines[1]).toBeEmptyDOMElement();

  await rerender({
    content: `${contentWithBlankLine}\`\`\`\nAfter the code.`,
    markdown: true,
    streaming: true,
    streamId: 'assistant-message-1',
  });

  await waitFor(() => expect(screen.getByText('After the code.')).toBeInTheDocument());
  expect(screen.getByText('Stable paragraph.')).toBe(stableParagraph);
  expect(container.querySelectorAll('[data-code-block]')).toHaveLength(1);
});
