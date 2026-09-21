import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import MessageComposer from '../../src/components/chat/MessageComposer.svelte';
import type { ChatCommand } from '../../src/lib/chatCommands';

beforeEach(() => {
  // jsdom has no layout; give the anchored popup a visible viewport and editor.
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 100, 600, 80));
  vi.spyOn(HTMLElement.prototype, 'getClientRects').mockReturnValue([new DOMRect(0, 100, 600, 80)] as unknown as DOMRectList);
  vi.spyOn(document.documentElement, 'clientWidth', 'get').mockReturnValue(1024);
  vi.spyOn(document.documentElement, 'clientHeight', 'get').mockReturnValue(768);
});

afterEach(() => vi.restoreAllMocks());

function commands(): ChatCommand[] {
  return [
    { name: '/new', description: 'Start a new chat', run: vi.fn() },
    { name: '/exit', description: 'End the session', run: vi.fn() },
  ];
}

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

test('selects slash commands with arrow keys, completes with Tab, and executes with Enter', async () => {
  const items = commands();
  const onSubmit = vi.fn();
  render(MessageComposer, { value: '', commands: items, onSubmit });
  const editor = screen.getByRole('textbox');
  await userEvent.type(editor, '/');
  expect(await screen.findByRole('listbox', { name: 'Chat commands' })).toBeInTheDocument();
  await fireEvent.keyDown(editor, { key: 'ArrowUp' });
  expect(screen.getByRole('option', { name: /\/exit/ })).toHaveAttribute('aria-selected', 'true');
  await fireEvent.keyDown(editor, { key: 'Tab' });
  await waitFor(() => expect(editor).toHaveTextContent('/exit'));
  expect(items[1].run).not.toHaveBeenCalled();
  expect(editor).toHaveFocus();
  await fireEvent.keyDown(editor, { key: 'Enter' });
  expect(items[1].run).toHaveBeenCalledOnce();
  expect(onSubmit).not.toHaveBeenCalled();
});

test('Escape dismisses suggestions and a complete command still runs from the submit button', async () => {
  const items = commands();
  const onSubmit = vi.fn();
  render(MessageComposer, { value: '/new', commands: items, submitDisabled: true, onSubmit });
  const editor = screen.getByRole('textbox');
  await fireEvent.focus(editor);
  await screen.findByRole('listbox');
  await fireEvent.keyDown(editor, { key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('listbox')).not.toBeInTheDocument());
  await fireEvent.click(screen.getByRole('button', { name: 'Run /new' }));
  expect(items[0].run).toHaveBeenCalledOnce();
  expect(onSubmit).not.toHaveBeenCalled();
});

test.each(['Enter', 'Tab', 'click'])('selecting /rename with %s completes the command before accepting a name', async (selection) => {
  const run = vi.fn();
  const onSubmit = vi.fn();
  render(MessageComposer, {
    value: '', commands: [{ name: '/rename', description: 'Rename the session', run }], onSubmit,
  });
  const editor = screen.getByRole('textbox');
  await userEvent.type(editor, '/ren');
  const option = await screen.findByRole('option', { name: /\/rename <name>/ });
  if (selection === 'click') await fireEvent.click(option);
  else await fireEvent.keyDown(editor, { key: selection });
  await waitFor(() => expect(editor).toHaveTextContent('/rename'));
  expect(screen.getByRole('button', { name: 'Run /rename' })).toBeDisabled();
  expect(run).not.toHaveBeenCalled();
  await userEvent.type(editor, 'Project planning');
  await fireEvent.keyDown(editor, { key: 'Enter' });
  expect(run).toHaveBeenCalledExactlyOnceWith('Project planning');
  expect(onSubmit).not.toHaveBeenCalled();
});

test('disabled commands cannot run or become ordinary messages', async () => {
  const items = commands();
  items[1].disabledReason = 'No current session';
  const onSubmit = vi.fn();
  render(MessageComposer, { value: '/exit', commands: items, onSubmit });
  const editor = screen.getByRole('textbox');
  await fireEvent.focus(editor);
  expect(await screen.findByRole('option', { name: /\/exit/ })).toBeDisabled();
  await fireEvent.keyDown(editor, { key: 'Enter' });
  await fireEvent.keyDown(editor, { key: 'Escape' });
  await fireEvent.keyDown(editor, { key: 'Enter' });
  expect(screen.getByRole('button', { name: 'Run /exit' })).toBeDisabled();
  expect(items[1].run).not.toHaveBeenCalled();
  expect(onSubmit).not.toHaveBeenCalled();
});

test.each(['/unknown', '/new details', 'Explain /exit', '/new\n/exit', '/tmp/project'])('sends %j as ordinary text', async (value) => {
  const items = commands();
  const onSubmit = vi.fn();
  render(MessageComposer, { value, commands: items, onSubmit });
  const editor = screen.getByRole('textbox');
  await fireEvent.focus(editor);
  expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  await fireEvent.keyDown(editor, { key: 'Enter' });
  expect(onSubmit).toHaveBeenCalledOnce();
  for (const item of items) expect(item.run).not.toHaveBeenCalled();
});

test('does not execute a command while confirming IME input or inserting a newline', async () => {
  const items = commands();
  const onSubmit = vi.fn();
  render(MessageComposer, { value: '/exit', commands: items, onSubmit });
  const editor = screen.getByRole('textbox');
  await fireEvent.focus(editor);
  await fireEvent.keyDown(editor, { key: 'Enter', isComposing: true });
  await fireEvent.keyDown(editor, { key: 'Enter', shiftKey: true });
  expect(items[1].run).not.toHaveBeenCalled();
  expect(onSubmit).not.toHaveBeenCalled();
});

test('executes a selected command in the fullscreen composer and closes the dialog', async () => {
  const items = commands();
  const onSubmit = vi.fn();
  render(MessageComposer, { value: '/', commands: items, fullscreen: true, onSubmit });
  await fireEvent.click(screen.getByRole('button', { name: 'Expand message composer' }));
  await screen.findByRole('dialog');
  const option = await screen.findByRole('option', { name: /\/exit/ });
  await fireEvent.click(option);
  expect(items[1].run).toHaveBeenCalledOnce();
  expect(onSubmit).not.toHaveBeenCalled();
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});
