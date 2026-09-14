import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, expect, test, vi } from 'vitest';
import FileMentionEditor from '../../src/lib/components/file-picker/FileMentionEditor.svelte';
import FileMentionHarness from './FileMentionHarness.svelte';
import { listWorkspaceFilePickerEntries } from '../../src/api/client';

vi.mock('../../src/api/client', () => ({
  listWorkspaceFilePickerEntries: vi.fn(),
}));

const files = [
  { path: 'src', name: 'src', kind: 'directory' },
  { path: 'src/main.rs', name: 'main.rs', kind: 'file' },
  { path: 'src/lib.rs', name: 'lib.rs', kind: 'file' },
];

beforeEach(() => {
  vi.mocked(listWorkspaceFilePickerEntries).mockResolvedValue({
    files,
    truncated: false,
    warnings: [],
  });
  Element.prototype.scrollIntoView = vi.fn();
});

function renderEditor(value = '') {
  const result = render(FileMentionEditor, {
    props: {
      value,
      workspaceId: 'workspace-1',
      placeholder: 'Message',
    },
  });
  return { ...result, editor: screen.getByRole('textbox') };
}

async function openPicker(editor: HTMLElement): Promise<void> {
  editor.focus();
  await userEvent.keyboard('@');
  await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(3));
}

test('opens file suggestions at a token boundary but not inside an email address', async () => {
  const { editor } = renderEditor();
  editor.focus();
  await userEvent.keyboard('test@example.com');

  expect(screen.queryByRole('listbox', { name: 'File suggestions' })).not.toBeInTheDocument();

  await userEvent.keyboard(' @');
  expect(await screen.findByRole('listbox', { name: 'File suggestions' })).toBeInTheDocument();
  await waitFor(() => expect(listWorkspaceFilePickerEntries).toHaveBeenCalledWith(
    'workspace-1',
    '',
    expect.objectContaining({ limit: 20, signal: expect.any(AbortSignal) }),
  ));
});

test('opens suggestions at the start of a new line', async () => {
  const { editor } = renderEditor();
  editor.focus();
  await userEvent.keyboard('first line');
  await fireEvent.keyDown(editor, { key: 'Enter', shiftKey: true });
  await userEvent.keyboard('@');

  await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(3));
});

test('shows loading and empty states while searching', async () => {
  let resolveSearch: ((result: { files: never[]; truncated: false; warnings: never[] }) => void) | undefined;
  vi.mocked(listWorkspaceFilePickerEntries).mockReturnValue(new Promise((resolve) => (resolveSearch = resolve)));
  const { editor } = renderEditor();
  editor.focus();
  await userEvent.keyboard('@');

  expect(await screen.findByText('Searching files…')).toBeInTheDocument();
  resolveSearch?.({ files: [], truncated: false, warnings: [] });
  expect(await screen.findByText('No matching files')).toBeInTheDocument();
});

test('navigates, scrolls, and dismisses file suggestions with the keyboard', async () => {
  const { editor } = renderEditor();
  await openPicker(editor);
  const options = screen.getAllByRole('option');
  expect(options[0]).toHaveAttribute('aria-selected', 'true');

  await fireEvent.keyDown(editor, { key: 'ArrowDown' });
  expect(options[1]).toHaveAttribute('aria-selected', 'true');
  expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' });

  await fireEvent.keyDown(editor, { key: 'Escape' });
  expect(screen.queryByRole('listbox', { name: 'File suggestions' })).not.toBeInTheDocument();
  await fireEvent.keyUp(editor, { key: 'Escape' });
  expect(screen.queryByRole('listbox', { name: 'File suggestions' })).not.toBeInTheDocument();
});

test('inserts the selected file identity as an atomic chip followed by a separator', async () => {
  render(FileMentionHarness);
  const editor = screen.getByRole('textbox');
  editor.focus();
  await userEvent.keyboard('open @');
  await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(3));

  await fireEvent.keyDown(editor, { key: 'ArrowDown' });
  await fireEvent.keyDown(editor, { key: 'Enter' });

  const chip = editor.querySelector('[data-type="mention"]');
  expect(chip).toHaveTextContent('@src/main.rs');
  expect(chip).toHaveAttribute('data-path', 'src/main.rs');
  expect(chip).toHaveAttribute('data-name', 'main.rs');
  expect(chip).toHaveAttribute('data-kind', 'file');
  expect(chip).toHaveAttribute('title', 'src/main.rs');
  expect(chip?.nextSibling?.textContent).toBe(' ');
  expect(screen.getByTestId('prompt-value').textContent).toBe('open @src/main.rs ');
  expect(screen.queryByRole('listbox', { name: 'File suggestions' })).not.toBeInTheDocument();

  await userEvent.keyboard('{Backspace}');
  await waitFor(() => expect(editor.querySelector('[data-type="mention"]')).not.toBeInTheDocument());
  expect(screen.getByTestId('prompt-value')).toHaveTextContent('open');
});

test('selects directory suggestions with Tab', async () => {
  render(FileMentionHarness);
  const editor = screen.getByRole('textbox');
  await openPicker(editor);

  await fireEvent.keyDown(editor, { key: 'Tab' });

  expect(editor.querySelector('[data-type="mention"]')).toHaveAttribute('data-kind', 'directory');
  expect(screen.getByTestId('prompt-value').textContent).toBe('@src ');
});

test('synchronizes external plain text without fabricating file identities', async () => {
  render(FileMentionHarness, { props: { initialValue: 'first line' } });
  const editor = screen.getByRole('textbox');

  await userEvent.click(screen.getByRole('button', { name: 'Replace draft' }));

  await waitFor(() => expect(editor.querySelector('br')).toBeInTheDocument());
  expect(editor.querySelector('[data-type="mention"]')).not.toBeInTheDocument();
  expect(screen.getByTestId('prompt-value').textContent).toBe('external @src/lib.rs\nnext line');
});
