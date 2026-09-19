import { mocks, session, turn, workspace } from './fixtures';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import type { CreateSessionResult } from '../../../src/api/types';

const NewChatPage = (await import('../../../src/pages/NewChatPage.svelte')).default;

test('guides first-time users to activate a workspace instead of showing an unusable composer', async () => {
  mocks.workspaces.set([]);
  mocks.browseWorkspaceRoot.mockResolvedValue({
    root_id: 'root-1',
    path: '',
    canonical_path: '/repo',
    parent_path: null,
    entries: [{ name: 'pontia', path: 'pontia', kind: 'directory', is_workspace: false }],
    warnings: [],
  });

  render(NewChatPage);

  expect(await screen.findByRole('heading', { name: 'Set up your first workspace' })).toBeInTheDocument();
  expect(screen.getByText(/workspace is the project directory/i)).toBeInTheDocument();
  expect(await screen.findByRole('button', { name: 'Activate pontia' })).toBeInTheDocument();
  expect(screen.queryByPlaceholderText('What should the agent do?')).not.toBeInTheDocument();
});

test('keeps first-time users in workspace setup until they continue explicitly', async () => {
  const user = userEvent.setup();
  const firstWorkspace = workspace();
  mocks.workspaces.set([]);
  mocks.browseWorkspaceRoot.mockResolvedValue({
    root_id: 'root-1',
    path: '',
    canonical_path: '/repo',
    parent_path: null,
    entries: [{ name: 'pontia', path: 'pontia', kind: 'directory', is_workspace: false }],
    warnings: [],
  });
  mocks.registerWorkspace.mockImplementation(async () => {
    mocks.workspaces.set([firstWorkspace]);
    return firstWorkspace;
  });

  render(NewChatPage);
  const continueButton = await screen.findByRole('button', { name: 'Continue to New Chat' });
  expect(continueButton).toBeDisabled();

  await user.click(await screen.findByRole('button', { name: 'Activate pontia' }));

  await waitFor(() => expect(mocks.registerWorkspace).toHaveBeenCalledWith({ root_id: 'root-1', path: 'pontia', name: 'pontia' }));
  expect(screen.getByRole('heading', { name: 'Set up your first workspace' })).toBeInTheDocument();
  expect(screen.queryByPlaceholderText('What should the agent do?')).not.toBeInTheDocument();
  expect(continueButton).toBeEnabled();

  await user.click(continueButton);

  expect(await screen.findByPlaceholderText('What should the agent do?')).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: 'Set up your first workspace' })).not.toBeInTheDocument();
});

test('explains how to configure Pontia when no workspace roots exist', async () => {
  mocks.workspaces.set([]);
  mocks.workspaceRoots.set([]);

  render(NewChatPage);

  expect(await screen.findByText('No workspace roots configured')).toBeInTheDocument();
  expect(screen.getByText('pontia init')).toBeInTheDocument();
  expect(screen.getByText(/restart Pontia/i)).toBeInTheDocument();
});

test('focuses the prompt only on the first entry to the new chat page', async () => {
  const firstPage = render(NewChatPage);

  const firstPrompt = await screen.findByPlaceholderText('What should the agent do?');
  await waitFor(() => expect(firstPrompt).toHaveFocus());
  firstPage.unmount();

  render(NewChatPage);
  const revisitedPrompt = await screen.findByPlaceholderText('What should the agent do?');
  expect(revisitedPrompt).not.toHaveFocus();
});

test('prefers the new chat workspace query parameter over the remembered workspace', async () => {
  window.history.pushState({}, '', '/dashboard?workspace=workspace-2');
  window.localStorage.setItem('pontia.chat.lastWorkspaceId', 'workspace-1');
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText('What should the agent do?');
  expect(screen.getByLabelText(/^Workspace$/i)).toHaveTextContent('sandbox');
  expect(window.localStorage.getItem('pontia.chat.lastWorkspaceId')).toBe('workspace-1');
});

test('updates the selected workspace when the mounted page query changes', async () => {
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText('What should the agent do?');
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(screen.getByLabelText(/^Workspace$/i)).toHaveTextContent('pontia');

  window.history.pushState({}, '', '/dashboard?workspace=workspace-2');
  window.dispatchEvent(new PopStateEvent('popstate'));

  await waitFor(() => expect(screen.getByLabelText(/^Workspace$/i)).toHaveTextContent('sandbox'));
});


test('remembers the selected new chat workspace after starting a chat', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-selected-workspace' });
  mocks.createSession.mockResolvedValue({ session: created, initial_turn: turn({ session_id: 'session-selected-workspace' }) } satisfies CreateSessionResult);
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText('What should the agent do?');
  const workspaceSelector = screen.getByLabelText(/^Workspace$/i);
  await user.click(workspaceSelector);
  await user.keyboard('{ArrowDown}{Enter}{Escape}');
  expect(workspaceSelector).toHaveTextContent('sandbox');
  document.body.style.pointerEvents = '';
  await user.type(screen.getByPlaceholderText('What should the agent do?'), 'Use sandbox');
  await user.click(screen.getByRole('button', { name: /start session/i }));

  await vi.waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith(expect.objectContaining({ workspace_id: 'workspace-2' })));
  expect(window.localStorage.getItem('pontia.chat.lastWorkspaceId')).toBe('workspace-2');
});


test('offers the implemented client and leaves submission disabled until a task is entered', async () => {
  render(NewChatPage);

  expect(await screen.findByRole('heading', { name: 'Start a session' })).toBeInTheDocument();
  const clients = screen.getByRole('group', { name: 'Agent client' });
  expect(within(clients).getAllByRole('button')).toHaveLength(1);
  expect(within(clients).getByRole('button', { name: 'pi' })).toHaveAttribute('aria-pressed', 'true');
  expect(screen.getByRole('button', { name: 'Start session' })).toBeDisabled();
  await userEvent.type(screen.getByRole('textbox'), 'Inspect this workspace');
  expect(screen.getByRole('button', { name: 'Start session' })).toBeEnabled();
});

test.each([true, false])('recovers from a workspace load error, with registered workspaces: %s', async (hasWorkspace) => {
  mocks.workspaces.set([]);
  mocks.workspacesError.set('Workspace request failed');
  render(NewChatPage);

  expect(await screen.findByRole('alert')).toHaveTextContent('Workspace request failed');
  expect(screen.getByRole('button', { name: 'Start session' })).toBeDisabled();
  mocks.loadWorkspaces.mockImplementationOnce(async () => {
    mocks.workspacesError.set(null);
    mocks.workspaces.set(hasWorkspace ? [workspace()] : []);
  });
  await userEvent.click(screen.getByRole('button', { name: 'Retry' }));

  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  if (hasWorkspace) {
    expect(screen.getByLabelText('Workspace')).toHaveTextContent('pontia');
  } else {
    expect(await screen.findByRole('heading', { name: 'Set up your first workspace' })).toBeInTheDocument();
  }
});

test('creates a session with initial prompt, workspace, and client then opens its chat', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-new' });
  mocks.createSession.mockResolvedValue({ session: created, initial_turn: turn({ session_id: 'session-new' }) } satisfies CreateSessionResult);
  render(NewChatPage);

  await user.type(screen.getByPlaceholderText('What should the agent do?'), 'Implement the dashboard chat flow');
  await fireEvent.click(screen.getByRole('button', { name: /start session/i }));

  await waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith({
    client_type: 'pi',
    workspace_id: 'workspace-1',
    title: 'Implement the dashboard chat flow',
    initial_task: { input: 'Implement the dashboard chat flow', metadata: { source: 'dashboard_chat' } },
    metadata: { source: 'dashboard_chat' },
  }));
  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-new');
});
