import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import MessageComposer from '../../src/components/chat/MessageComposer.svelte';

test('backslash plus Enter inserts a newline instead of submitting', async () => {
  const onSubmit = vi.fn();
  render(MessageComposer, { props: { value: '', onSubmit } });
  const editor = screen.getByRole('textbox');
  await userEvent.type(editor, 'first line\\');

  expect(await fireEvent.keyDown(editor, { key: 'Enter' })).toBe(false);

  await waitFor(() => expect(editor.querySelector('br:not(.ProseMirror-trailingBreak)')).toBeInTheDocument());
  expect(editor).toHaveTextContent('first line');
  expect(onSubmit).not.toHaveBeenCalled();
});
