import { mocks, session, turn, workspace } from './fixtures';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
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
  expect(screen.queryByPlaceholderText('Ask the agent to implement, inspect, or explain something…')).not.toBeInTheDocument();
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
  expect(screen.queryByPlaceholderText('Ask the agent to implement, inspect, or explain something…')).not.toBeInTheDocument();
  expect(continueButton).toBeEnabled();

  await user.click(continueButton);

  expect(await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…')).toBeInTheDocument();
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

  const firstPrompt = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  await waitFor(() => expect(firstPrompt).toHaveFocus());
  firstPage.unmount();

  render(NewChatPage);
  const revisitedPrompt = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
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

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(screen.getByLabelText(/^Workspace$/i)).toHaveTextContent('sandbox');
  expect(window.localStorage.getItem('pontia.chat.lastWorkspaceId')).toBe('workspace-1');
});

test('updates the selected workspace when the mounted page query changes', async () => {
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
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

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  const workspaceSelector = screen.getByLabelText(/^Workspace$/i);
  await user.click(workspaceSelector);
  await user.keyboard('{ArrowDown}{Enter}{Escape}');
  expect(workspaceSelector).toHaveTextContent('sandbox');
  document.body.style.pointerEvents = '';
  await user.type(screen.getByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'Use sandbox');
  await user.click(screen.getByRole('button', { name: /start chat/i }));

  await vi.waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith(expect.objectContaining({ workspace_id: 'workspace-2' })));
  expect(window.localStorage.getItem('pontia.chat.lastWorkspaceId')).toBe('workspace-2');
});


test('shows active agents and opens their chats from the overview', async () => {
  mocks.sessions.set([
    session({ session_id: 'session-working', title: 'Implement overview', state: 'busy', updated_at: '2026-05-14T00:01:00Z' }),
    session({ session_id: 'session-idle', title: 'Review changes', state: 'idle' }),
    session({ session_id: 'session-exited', title: 'Old session', state: 'exited' }),
  ]);

  render(NewChatPage);

  expect(await screen.findByText('Implement overview')).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: 'Overview' })).not.toBeInTheDocument();
  expect(screen.getByText('Review changes')).toBeInTheDocument();
  expect(screen.queryByText('Old session')).not.toBeInTheDocument();

  await fireEvent.click(screen.getByRole('button', { name: 'Open Implement overview, Working' }));
  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-working');
});

test('does not show an empty-state placeholder when there are no active agents', async () => {
  mocks.sessions.set([
    session({ session_id: 'session-exited', state: 'exited' }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(screen.queryByRole('heading', { name: 'Overview' })).not.toBeInTheDocument();
  expect(screen.queryByRole('region', { name: 'Active agents' })).not.toBeInTheDocument();
  expect(screen.queryByText('No active agents')).not.toBeInTheDocument();
});

test('filters the overview by workspace and syncs the new chat target', async () => {
  const user = userEvent.setup();
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);
  mocks.sessions.set([
    session({ session_id: 'session-pontia', title: 'Pontia agent', workspace_id: 'workspace-1' }),
    session({ session_id: 'session-sandbox', title: 'Sandbox agent', workspace_id: 'workspace-2' }),
  ]);

  render(NewChatPage);

  const overviewWorkspace = await screen.findByRole('button', { name: 'Overview workspace' });
  expect(overviewWorkspace).toHaveTextContent('All workspaces');
  await user.click(overviewWorkspace);
  await user.keyboard('{ArrowDown}{ArrowDown}{Enter}{Escape}');

  expect(mocks.navigate).toHaveBeenCalledWith('/', { workspace: 'workspace-2' });
  expect(screen.queryByText('Pontia agent')).not.toBeInTheDocument();
  expect(screen.getByText('Sandbox agent')).toBeInTheDocument();
  expect(screen.getByLabelText(/^Workspace$/i)).toHaveTextContent('sandbox');
});

test('renders a bottom-aligned prompt input with inline workspace and client selectors on the bare chat route', async () => {
  render(NewChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(promptInput).toHaveValue('');
  expect(screen.queryByRole('heading', { name: /new chat/i })).not.toBeInTheDocument();
  expect(screen.queryByText('Start a new agent session from a prompt, workspace, and client.')).not.toBeInTheDocument();
  expect(screen.getByText('Start a new agent session from')).toBeInTheDocument();
  expect(screen.getByText(', use')).toBeInTheDocument();
  const panel = screen.getByTestId('new-chat-panel');
  const pageSection = panel.closest('section');
  expect(pageSection).toHaveClass('min-h-[calc(100svh-5.5rem)]');
  expect(pageSection).toHaveClass('md:min-h-[calc(100svh-6.5rem)]');
  expect(pageSection?.className).not.toContain('100vh');
  expect(panel).toHaveClass('justify-end');
  expect(panel).not.toHaveClass('justify-center');
  expect(panel).toContainElement(promptInput);
  expect(screen.queryByText(/Enter the first prompt/i)).not.toBeInTheDocument();
  expect(screen.queryByText(/^Prompt$/i)).not.toBeInTheDocument();
  const workspaceSelector = screen.getByLabelText(/^Workspace$/i);
  const clientSelector = screen.getByLabelText(/client/i);
  expect(workspaceSelector).toHaveTextContent('pontia');
  expect(clientSelector).toHaveTextContent('pi');
  expect(workspaceSelector).toHaveClass('rounded-md');
  expect(workspaceSelector).not.toHaveClass('rounded-full');
  expect(clientSelector).toHaveClass('rounded-md');
  expect(clientSelector).not.toHaveClass('rounded-full');
  expect(screen.queryByLabelText(/profile/i)).not.toBeInTheDocument();
  expect(mocks.loadSessionDetail).not.toHaveBeenCalled();
});



test('creates a session with initial prompt, workspace, and client then opens its chat', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-new' });
  mocks.createSession.mockResolvedValue({ session: created, initial_turn: turn({ session_id: 'session-new' }) } satisfies CreateSessionResult);
  render(NewChatPage);

  await user.type(screen.getByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'Implement the dashboard chat flow');
  await fireEvent.click(screen.getByRole('button', { name: /start chat/i }));

  await waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith({
    client_type: 'pi',
    workspace_id: 'workspace-1',
    title: 'Implement the dashboard chat flow',
    initial_task: { input: 'Implement the dashboard chat flow', metadata: { source: 'dashboard_chat' } },
    metadata: { source: 'dashboard_chat' },
  }));
  expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-new');
});

