import { mocks, session, timelineStateValue } from './fixtures';
import { render, screen, waitFor } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import SessionChatPage from '../../../src/pages/SessionChatPage.svelte';

test.each([
  ['network', 'Unable to connect'],
  ['authentication', 'Authentication required'],
  ['not_found', 'Session not found'],
  ['request', 'Unable to load session'],
])('presents a %s failure distinctly', async (kind, heading) => {
  mocks.sessions.set([]);
  mocks.loadedSessions = [];
  mocks.loadSessionDetail.mockImplementation(async () => {
    mocks.sessionDetailErrorKind.set(kind);
    mocks.sessionDetailError.set('Snapshot request failed');
    return null;
  });
  render(SessionChatPage, { routeSessionId: 'session-1' });
  expect(await screen.findByText(heading)).toBeInTheDocument();
  if (kind !== 'not_found') expect(screen.queryByText('Session not found')).not.toBeInTheDocument();
});

test.each(['pi', 'codex'] as const)('restores %s controls and output subscription after an initially empty detail recovers', async (clientType) => {
  mocks.sessions.set([]);
  mocks.loadedSessions = [];
  mocks.loadSessionDetail.mockImplementation(async () => {
    mocks.sessionDetailErrorKind.set('network');
    mocks.sessionDetailError.set('Failed to fetch');
    return null;
  });
  const page = render(SessionChatPage, { routeSessionId: 'session-1' });
  await screen.findByText('Unable to connect');

  const recovered = session({ client_type: clientType, capabilities: { timeline: true, accept_task: true, stream_output: true } });
  mocks.sessionDetailError.set(null);
  mocks.sessionDetailErrorKind.set(null);
  mocks.sessionDetail.set({ session: recovered, turns: [], inboxMessages: [], events: [] });
  mocks.timelineState.set(timelineStateValue({ sessionId: 'session-1', status: 'empty', mode: 'linear' }));

  expect(await screen.findByPlaceholderText('Continue the thread…')).toBeInTheDocument();
  await waitFor(() => expect(mocks.liveOutputListeners.has('session-1')).toBe(true));
  expect(mocks.dashboardEventListeners.size).toBe(1);
  expect(screen.queryByText('Unable to connect')).not.toBeInTheDocument();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
  expect(mocks.recoverInboxSubmission).not.toHaveBeenCalled();
  page.unmount();
  expect(mocks.liveOutputListeners.size).toBe(0);
  expect(mocks.dashboardEventListeners.size).toBe(0);
});
