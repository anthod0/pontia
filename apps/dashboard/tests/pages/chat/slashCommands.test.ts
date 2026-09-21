import { mocks, session } from './fixtures';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { get } from 'svelte/store';
import { afterEach, expect, test, vi } from 'vitest';
import { chatDraft } from '../../../src/stores/chatDraft';
import type { SessionView } from '../../../src/api/types';
import * as api from '../../../src/api/client';

afterEach(() => vi.restoreAllMocks());

const SessionChatPage = (await import('../../../src/pages/SessionChatPage.svelte')).default;
const NewChatPage = (await import('../../../src/pages/NewChatPage.svelte')).default;

function renderChat(value: string, overrides: Partial<SessionView> = {}) {
  const selected = session({ workspace_id: 'workspace-1', ...overrides, capabilities: { accept_task: true, timeline: true, ...overrides.capabilities } });
  mocks.pathParams = { sessionId: selected.session_id };
  window.history.pushState({}, '', `/dashboard/chat/${selected.session_id}`);
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
  chatDraft.set(value);
  render(SessionChatPage);
  return selected;
}

test.each(['idle', 'error', 'exited'])('/new opens the current workspace without starting or ending a session in state %s', async (state) => {
  renderChat('/new', { state, capabilities: { accept_task: false } });
  await fireEvent.click(await screen.findByRole('button', { name: 'Run /new' }));
  expect(mocks.navigate).toHaveBeenCalledWith('/', { workspace: 'workspace-1' });
  expect(get(chatDraft)).toBe('');
  expect(mocks.createSession).not.toHaveBeenCalled();
  expect(mocks.terminateSession).not.toHaveBeenCalled();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
});

test('/exit terminates a busy session once, keeps the page, and waits for reported state', async () => {
  let finish!: () => void;
  mocks.terminateSession.mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; }));
  const selected = renderChat('/exit', { state: 'busy', capabilities: { accept_task: false } });
  const button = await screen.findByRole('button', { name: 'Run /exit' });
  await fireEvent.click(button);
  expect(button).toBeDisabled();
  await fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });
  expect(mocks.terminateSession).toHaveBeenCalledExactlyOnceWith(selected.session_id);
  finish();
  await waitFor(() => expect(get(chatDraft)).toBe(''));
  expect(mocks.sessionDetail.get()?.session.state).toBe('busy');
  expect(mocks.navigate).not.toHaveBeenCalled();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
});

test('failed /exit preserves the command for retry and shows the error', async () => {
  mocks.terminateSession.mockRejectedValueOnce(new Error('Exit request failed'));
  const selected = renderChat('/exit');
  await fireEvent.click(await screen.findByRole('button', { name: 'Run /exit' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Exit request failed');
  expect(get(chatDraft)).toBe('/exit');
  expect(screen.getByRole('button', { name: 'Run /exit' })).toBeEnabled();
  await fireEvent.click(screen.getByRole('button', { name: 'Run /exit' }));
  await waitFor(() => expect(get(chatDraft)).toBe(''));
  expect(mocks.terminateSession).toHaveBeenLastCalledWith(selected.session_id);
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
});

test.each(['exited', 'error'])('/exit is unavailable for an already terminal %s session', async (state) => {
  renderChat('/exit', { state });
  expect(await screen.findByRole('button', { name: 'Run /exit' })).toBeDisabled();
  await fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });
  expect(mocks.terminateSession).not.toHaveBeenCalled();
  expect(mocks.resumeSession).not.toHaveBeenCalled();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
});

test('new chat disables /exit and /new clears the prompt while preserving workspace selection', async () => {
  chatDraft.set('/exit');
  render(NewChatPage);
  expect(await screen.findByRole('button', { name: 'Run /exit' })).toBeDisabled();
  await fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });
  expect(mocks.createSession).not.toHaveBeenCalled();
  chatDraft.set('/new');
  await fireEvent.click(await screen.findByRole('button', { name: 'Run /new' }));
  expect(get(chatDraft)).toBe('');
  expect(mocks.navigate).toHaveBeenCalledWith('/', { workspace: 'workspace-1' });
  expect(mocks.createSession).not.toHaveBeenCalled();
});

test('file mentions keep their keyboard selection before ordinary message submission', async () => {
  vi.spyOn(api, 'listWorkspaceFilePickerEntries').mockResolvedValue({
    files: [{ path: 'src/main.rs', name: 'main.rs', kind: 'file' }], truncated: false, warnings: [],
  });
  renderChat('');
  const editor = await screen.findByRole('textbox');
  await userEvent.type(editor, '@src');
  await screen.findByRole('listbox', { name: 'File suggestions' });
  await screen.findByRole('option', { name: '@src/main.rs' });
  await fireEvent.keyDown(editor, { key: 'Enter' });
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
  await fireEvent.keyDown(editor, { key: 'Enter' });
  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-1', expect.objectContaining({
    input: '@src/main.rs',
  })));
});
