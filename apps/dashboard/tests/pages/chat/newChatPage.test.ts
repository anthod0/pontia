import { mocks, session, turn, workspace } from './fixtures';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import type { CreateSessionResult } from '../../../src/api/types';

const NewChatPage = (await import('../../../src/pages/NewChatPage.svelte')).default;

test('prefers the new chat workspace query parameter over the remembered workspace', async () => {
  window.history.pushState({}, '', '/dashboard/chat?workspace=workspace-2');
  window.localStorage.setItem('pontia.chat.lastWorkspaceId', 'workspace-1');
  mocks.workspaces.set([
    workspace({ workspace_id: 'workspace-1', name: 'pontia' }),
    workspace({ workspace_id: 'workspace-2', name: 'sandbox', canonical_path: '/repo/sandbox', display_path: '~/repo/sandbox' }),
  ]);

  render(NewChatPage);

  await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(screen.getByLabelText(/workspace/i)).toHaveTextContent('sandbox');
  expect(window.localStorage.getItem('pontia.chat.lastWorkspaceId')).toBe('workspace-1');
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
  const workspaceSelector = screen.getByLabelText(/workspace/i);
  await user.click(workspaceSelector);
  await user.keyboard('{ArrowDown}{Enter}{Escape}');
  expect(workspaceSelector).toHaveTextContent('sandbox');
  document.body.style.pointerEvents = '';
  await user.type(screen.getByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'Use sandbox');
  await user.click(screen.getByRole('button', { name: /start chat/i }));

  await vi.waitFor(() => expect(mocks.createSession).toHaveBeenCalledWith(expect.objectContaining({ workspace_id: 'workspace-2' })));
  expect(window.localStorage.getItem('pontia.chat.lastWorkspaceId')).toBe('workspace-2');
});


test('renders a clean centered prompt input with inline workspace and client selectors on the bare chat route', async () => {
  render(NewChatPage);

  const promptInput = await screen.findByPlaceholderText('Ask the agent to implement, inspect, or explain something…');
  expect(promptInput).toHaveValue('');
  expect(screen.queryByRole('heading', { name: /new chat/i })).not.toBeInTheDocument();
  expect(screen.queryByText('Start a new agent session from a prompt, workspace, and client.')).not.toBeInTheDocument();
  expect(screen.getByText('Start a new agent session from')).toBeInTheDocument();
  expect(screen.getByText(', use')).toBeInTheDocument();
  const centeredPanel = screen.getByTestId('new-chat-centered-panel');
  const pageSection = centeredPanel.closest('section');
  expect(pageSection).toHaveClass('min-h-[calc(100svh-5.5rem)]');
  expect(pageSection).toHaveClass('md:min-h-[calc(100svh-6.5rem)]');
  expect(pageSection?.className).not.toContain('100vh');
  expect(centeredPanel).toHaveClass('justify-center');
  expect(centeredPanel).toContainElement(promptInput);
  expect(screen.queryByText(/Enter the first prompt/i)).not.toBeInTheDocument();
  expect(screen.queryByText(/^Prompt$/i)).not.toBeInTheDocument();
  const workspaceSelector = screen.getByLabelText(/workspace/i);
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

