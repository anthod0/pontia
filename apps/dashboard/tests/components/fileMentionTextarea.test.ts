import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import FileMentionTextarea from '../../src/lib/components/file-picker/FileMentionTextarea.svelte';
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
  vi.useFakeTimers();
  vi.mocked(listWorkspaceFilePickerEntries).mockResolvedValue({
    files,
    truncated: false,
    warnings: [],
  });
  Element.prototype.scrollIntoView = vi.fn();
});

function renderOpenPicker() {
  render(FileMentionTextarea, {
    props: {
      value: '@',
      workspaceId: 'workspace-1',
      placeholder: 'Message',
    },
  });
  return screen.getByPlaceholderText('Message');
}

test('keeps keyboard selection after arrow keyup refreshes the textarea', async () => {
  const textarea = renderOpenPicker();
  await fireEvent.focus(textarea);
  await vi.advanceTimersByTimeAsync(150);

  await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(3));
  const options = screen.getAllByRole('option');
  expect(options[0]).toHaveAttribute('aria-selected', 'true');

  await fireEvent.keyDown(textarea, { key: 'ArrowDown' });
  expect(options[1]).toHaveAttribute('aria-selected', 'true');

  await fireEvent.keyUp(textarea, { key: 'ArrowDown' });
  expect(options[1]).toHaveAttribute('aria-selected', 'true');
});

test('scrolls the selected file suggestion into view after keyboard navigation', async () => {
  const textarea = renderOpenPicker();
  await fireEvent.focus(textarea);
  await vi.advanceTimersByTimeAsync(150);

  await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(3));
  await fireEvent.keyDown(textarea, { key: 'ArrowDown' });
  await Promise.resolve();

  expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' });
});

test('keeps file suggestions closed after escape keyup', async () => {
  const textarea = renderOpenPicker();
  await fireEvent.focus(textarea);
  await vi.advanceTimersByTimeAsync(150);

  await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(3));
  await fireEvent.keyDown(textarea, { key: 'Escape' });
  expect(screen.queryByRole('listbox', { name: 'File suggestions' })).not.toBeInTheDocument();

  await fireEvent.keyUp(textarea, { key: 'Escape' });
  expect(screen.queryByRole('listbox', { name: 'File suggestions' })).not.toBeInTheDocument();
});
