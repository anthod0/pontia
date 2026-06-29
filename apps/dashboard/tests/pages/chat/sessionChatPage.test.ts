import { inboxMessage, mocks, session, timelineItemsFromTurns, turn, workspace } from './fixtures';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import type { CreateSessionResult } from '../../../src/api/types';

const NewChatPage = (await import('../../../src/pages/NewChatPage.svelte')).default;
const SessionChatPage = (await import('../../../src/pages/SessionChatPage.svelte')).default;

class TestIntersectionObserver implements IntersectionObserver {
  static instances: TestIntersectionObserver[] = [];

  readonly root: Element | Document | null = null;
  readonly rootMargin = '0px';
  readonly thresholds = [0.01];
  private observedElement: Element | null = null;

  constructor(private readonly callback: IntersectionObserverCallback) {
    TestIntersectionObserver.instances.push(this);
  }

  observe(element: Element): void {
    this.observedElement = element;
  }

  unobserve(): void {}

  disconnect(): void {}

  takeRecords(): IntersectionObserverEntry[] { return []; }

  trigger(isIntersecting: boolean): void {
    if (!this.observedElement) return;
    this.callback([{ isIntersecting, target: this.observedElement } as IntersectionObserverEntry], this);
  }
}

function installIntersectionObserverMock(): void {
  TestIntersectionObserver.instances = [];
  Object.defineProperty(window, 'IntersectionObserver', { configurable: true, writable: true, value: TestIntersectionObserver });
  Object.defineProperty(globalThis, 'IntersectionObserver', { configurable: true, writable: true, value: TestIntersectionObserver });
}

async function triggerLatestBottomIntersection(isIntersecting: boolean): Promise<void> {
  await waitFor(() => expect(TestIntersectionObserver.instances.length).toBeGreaterThan(0));
  TestIntersectionObserver.instances.at(-1)?.trigger(isIntersecting);
}

test('opens new chat from a session menu with the current workspace query parameter', async () => {
  const selected = session({ session_id: 'session-2', workspace_id: 'workspace-2' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  await fireEvent.click(await screen.findByRole('button', { name: /advanced session controls/i }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: /new chat/i }));

  expect(mocks.navigate).toHaveBeenCalledWith('/chat', { workspace: 'workspace-2' });
});


test('shows busy agent status with an interrupt action when supported', async () => {
  const busySession = session({ state: 'busy', current_turn_id: 'turn-1', capabilities: { interrupt: true } });
  mocks.loadedSessions = [busySession];
  mocks.sessions.set([busySession]);
  mocks.sessionDetail.set({ session: busySession, turns: [turn({ state: 'running', output: null, completed_at: null })], inboxMessages: [], events: [], artifacts: [] });
  mocks.pathParams = { sessionId: 'session-1' };
  window.history.pushState({}, '', '/dashboard/chat/session-1');

  render(SessionChatPage);

  expect(await screen.findByLabelText('Agent status: Agent working')).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button', { name: /interrupt agent/i }));
  await waitFor(() => expect(mocks.interruptSession).toHaveBeenCalledWith('session-1'));
});


test('renames the selected chat session from advanced controls', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', title: 'Old title' });
  const renamed = session({ session_id: 'session-2', title: 'New title' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.updateSessionTitle.mockResolvedValue(renamed);
  render(SessionChatPage);

  await fireEvent.click(await screen.findByRole('button', { name: /advanced session controls/i }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: /rename session/i }));
  const titleInput = await screen.findByLabelText(/session title/i);
  await user.clear(titleInput);
  await user.type(titleInput, 'New title');
  await user.click(screen.getByRole('button', { name: /rename session/i }));

  await waitFor(() => expect(mocks.updateSessionTitle).toHaveBeenCalledWith('session-2', 'New title'));
});


test('shows the initial prompt immediately after starting a chat while timeline is empty', async () => {
  const user = userEvent.setup();
  const created = session({ session_id: 'session-new', state: 'busy', current_turn_id: 'turn-new' });
  const initialTurn = turn({
    turn_id: 'turn-new',
    session_id: 'session-new',
    state: 'running',
    input: { summary: 'hi' },
    output: null,
    completed_at: null,
  });
  mocks.createSession.mockImplementation(async () => {
    mocks.sessions.set([created]);
    mocks.sessionDetail.set({ session: created, turns: [initialTurn], inboxMessages: [], events: [], artifacts: [] });
    return { session: created, initial_turn: initialTurn } satisfies CreateSessionResult;
  });
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    mocks.timelineState.set({
      sessionId,
      bindingId: null,
      items: [],
      headCursor: null,
    tailCursor: null,
            sourceId: null,
      hasMore: false,
            loading: false,
      refreshing: false,
      error: null,
    });
    return null;
  });
  render(NewChatPage);

  await user.type(screen.getByPlaceholderText('Ask the agent to implement, inspect, or explain something…'), 'hi');
  await fireEvent.click(screen.getByRole('button', { name: /start chat/i }));

  await waitFor(() => expect(mocks.navigate).toHaveBeenCalledWith('/chat/session-new'));

  cleanup();
  window.history.pushState({}, '', '/dashboard/chat/session-new');
  mocks.pathParams = { sessionId: 'session-new' };
  render(SessionChatPage);

  const userMessage = await screen.findByText('hi');
  const userContent = userMessage.closest('[data-role="user"]')?.firstElementChild;
  expect(userContent).toHaveClass('group-[.is-user]:bg-secondary');
  expect(userContent).toHaveClass('group-[.is-user]:text-foreground');
  expect(userContent).not.toHaveClass('group-[.is-user]:bg-primary');
  expect(userContent).not.toHaveClass('group-[.is-user]:text-primary-foreground');
  expect(screen.queryByText('No messages yet')).not.toBeInTheDocument();
});


test('falls back to projected turns when a TUI-launched session timeline is not ready yet', async () => {
  const selected = session({ session_id: 'session-tui', state: 'busy', current_turn_id: 'turn-tui' });
  const activeTurn = turn({
    turn_id: 'turn-tui',
    session_id: 'session-tui',
    state: 'running',
    input: { summary: 'typed in tui' },
    output: null,
    completed_at: null,
  });
  window.history.pushState({}, '', '/dashboard/chat/session-tui');
  mocks.pathParams = { sessionId: 'session-tui' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [activeTurn], inboxMessages: [], events: [], artifacts: [] });
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    mocks.timelineState.set({
      sessionId,
      bindingId: null,
      items: [],
      headCursor: null,
      tailCursor: null,
      sourceId: null,
      hasMore: false,
      loading: true,
      refreshing: false,
      error: null,
    });
    return null;
  });

  render(SessionChatPage);

  expect(await screen.findByText('typed in tui')).toBeInTheDocument();
  expect(screen.queryByText('No messages yet')).not.toBeInTheDocument();
  expect(screen.queryByText('Loading conversation…')).not.toBeInTheDocument();
});


test('shows workspace git status in the selected chat composer summary', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle', workspace_id: 'workspace-1', workspace: '/repo/pontia', handle: null });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.workspaces.set([workspace({ workspace_id: 'workspace-1', name: 'project', canonical_path: '/repo/pontia', display_path: '~/repo/pontia' })]);
  mocks.workspaceGitStatuses.set({
    'workspace-1': {
      workspace_id: 'workspace-1',
      repo_root: '/repo/pontia',
      branch: 'main',
      upstream: 'origin/main',
      ahead: 1,
      behind: 2,
      staged_count: 3,
      unstaged_count: 4,
      untracked_count: 5,
      conflicted_count: 6,
      clean: false,
      state: 'observed',
      failure: null,
      observed_at: '2026-05-14T01:30:00Z',
      updated_at: '2026-05-14T01:30:00Z',
    },
  });

  render(SessionChatPage);

  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  const sessionDetailsButton = await screen.findByRole('button', { name: 'Session details: project · pi · main · dirty' });
  expect(sessionDetailsButton).toHaveTextContent('project · main ↑1 ↓2 +3 ~4 ?5 !6 · pi');
  expect(within(sessionDetailsButton).getByText('↑1')).toHaveClass('text-blue-600');
});


test('scrolls to the document bottom after entering a selected chat', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
  scrollTo.mockRestore();
});


test('scrolls to the settled document bottom when switching chats through SPA navigation', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const originalScrollHeight = Object.getOwnPropertyDescriptor(document.documentElement, 'scrollHeight')
    ?? Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollHeight');
  const firstSession = session({ session_id: 'session-1', state: 'idle' });
  const secondSession = session({ session_id: 'session-2', state: 'idle' });
  let layoutPasses = 0;

  try {
    Object.defineProperty(document.documentElement, 'scrollHeight', {
      configurable: true,
      get: () => {
        layoutPasses += 1;
        return layoutPasses < 2 ? 2048 : 4096;
      },
    });
    window.history.pushState({}, '', '/dashboard/chat/session-1');
    mocks.pathParams = { sessionId: 'session-1' };
    mocks.loadedSessions = [firstSession, secondSession];
    mocks.sessions.set([firstSession, secondSession]);
    mocks.sessionDetail.set({ session: firstSession, turns: [turn({ session_id: 'session-1' })], inboxMessages: [], events: [], artifacts: [] });
    mocks.loadSessionDetail.mockImplementation(async (sessionId: string) => {
      const selected = sessionId === 'session-2' ? secondSession : firstSession;
      mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: sessionId })], inboxMessages: [], events: [], artifacts: [] });
      return null;
    });

    render(SessionChatPage);
    await waitFor(() => expect(scrollTo).toHaveBeenCalled());
    scrollTo.mockClear();

    layoutPasses = 0;
    window.history.pushState({}, '', '/dashboard/chat/session-2');
    mocks.pathParams = { sessionId: 'session-2' };
    window.dispatchEvent(new PopStateEvent('popstate'));

    await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
  } finally {
    scrollTo.mockRestore();
    mocks.loadSessionDetail.mockImplementation(async () => null);
    if (originalScrollHeight) Object.defineProperty(document.documentElement, 'scrollHeight', originalScrollHeight);
    else delete (document.documentElement as HTMLElement & { scrollHeight?: number }).scrollHeight;
  }
});


test('does not load earlier chat history before the initial selected chat scroll settles', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    headCursor: 'older-cursor',
    tailCursor: 'tail-cursor',
    sourceId: 'source-1',
    hasMore: true,
    loading: false,
    refreshing: false,
    error: null,
  });

  render(SessionChatPage);

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 40 });
  window.dispatchEvent(new Event('scroll'));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalledWith('session-2', { mode: 'more' });
  scrollTo.mockRestore();
});


test('loads earlier chat history when the chat scroll reaches the top', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    headCursor: 'older-cursor',
    tailCursor: 'tail-cursor',
        sourceId: 'source-1',
    hasMore: true,
        loading: false,
    refreshing: false,
    error: null,
  });

  render(SessionChatPage);

  expect(screen.queryByRole('button', { name: /load earlier messages/i })).not.toBeInTheDocument();

  Object.defineProperty(window, 'scrollY', { configurable: true, value: 40 });
  window.dispatchEvent(new WheelEvent('wheel', { deltaY: -100 }));
  window.dispatchEvent(new Event('scroll'));

  await waitFor(() => expect(mocks.loadSessionTimeline).toHaveBeenCalledWith('session-2', { mode: 'more' }));
});


test('refreshes an already-loaded selected chat through the tail cursor without rebuilding loaded history', async () => {
  const selected = session({ session_id: 'session-2', state: 'running' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns([
      turn({ turn_id: 'turn-older', session_id: 'session-2', input: { summary: 'older question' }, output: { summary: 'older answer' } }),
      turn({ turn_id: 'turn-latest', session_id: 'session-2', input: { summary: 'latest question' }, output: { summary: 'latest answer' } }),
    ]),
    headCursor: 'older-cursor',
    tailCursor: 'tail-cursor',
        sourceId: 'source-1',
    hasMore: true,
        loading: false,
    refreshing: false,
    error: null,
  });

  render(SessionChatPage);

  await waitFor(() => expect(mocks.handleTimelineMessageUpdated).toHaveBeenCalledWith('session-2'));
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalledWith('session-2', { mode: 'rebuild' });
  expect(mocks.resetTimelineState).not.toHaveBeenCalledWith('session-2');
  await waitFor(() => expect(mocks.dashboardEventListeners.size).toBe(1));
  mocks.loadSessionDetail.mockClear();
  mocks.loadSessionTimeline.mockClear();
  mocks.handleTimelineMessageUpdated.mockClear();
  await new Promise((resolve) => setTimeout(resolve, 0));
  mocks.loadSessionTimeline.mockClear();
  mocks.handleTimelineMessageUpdated.mockClear();
  mocks.timelineState.set({
    ...mocks.timelineState.get(),
    sessionId: 'session-2',
    items: timelineItemsFromTurns([
      turn({ turn_id: 'turn-older', session_id: 'session-2', input: { summary: 'older question' }, output: { summary: 'older answer' } }),
      turn({ turn_id: 'turn-latest', session_id: 'session-2', input: { summary: 'latest question' }, output: { summary: 'latest answer' } }),
    ]),
  });

  window.dispatchEvent(new Event('focus'));

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2', { showLoading: false }));
  expect(mocks.handleTimelineMessageUpdated).toHaveBeenCalledWith('session-2');
});


test('coalesces bursty selected-session idle events into one git status refresh', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle', workspace_id: 'workspace-1' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  await waitFor(() => expect(mocks.dashboardEventListeners.size).toBe(1));
  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  mocks.refreshWorkspaceGitStatus.mockClear();

  const idleEvent = (eventId: string, type: string) => ({
    kind: 'session_event' as const,
    id: eventId,
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: eventId,
      session_id: 'session-2',
      turn_id: null,
      source: 'runtime',
      type,
      time: '2026-05-14T00:00:00Z',
      payload: {},
    },
  });

  for (const listener of mocks.dashboardEventListeners) {
    listener(idleEvent('evt-ready', 'session.ready'));
    listener(idleEvent('evt-completed', 'turn.completed'));
    listener(idleEvent('evt-failed', 'turn.failed'));
  }

  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledTimes(1));
  expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1');
});


test('does not toast transient network errors from automatic chat refreshes', async () => {
  const selected = session({ session_id: 'session-2', state: 'running' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    headCursor: null,
    tailCursor: null,
        sourceId: 'source-1',
    hasMore: false,
        loading: false,
    refreshing: false,
    error: null,
  });

  render(SessionChatPage);

  await waitFor(() => expect(mocks.handleTimelineMessageUpdated).toHaveBeenCalledWith('session-2'));
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalled();
  mocks.toastError.mockClear();

  mocks.sessionDetailError.set('Failed to fetch');
  mocks.timelineState.set({ ...mocks.timelineState.get(), error: 'net::ERR_NETWORK_CHANGED' });
  mocks.sessionsError.set('NetworkError when attempting to fetch resource.');

  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(mocks.toastError).not.toHaveBeenCalled();
});


test('mobile composer resize button opens a fullscreen follow-up composer sharing the current input', async () => {
  const originalMatchMedia = window.matchMedia;
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: query.includes('max-width'),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  try {
    render(SessionChatPage);

    const composerInput = await screen.findByPlaceholderText('Send a follow-up message…');
    await user.type(composerInput, 'mobile draft');
    await fireEvent.click(screen.getByRole('button', { name: 'Expand message composer' }));

    const fullscreenComposer = screen.getByRole('dialog', { name: 'Expanded message composer' });
    const fullscreenInput = within(fullscreenComposer).getByPlaceholderText('Send a follow-up message…');
    expect(fullscreenInput).toHaveValue('mobile draft');
    expect(fullscreenInput).toHaveClass('h-full');
    expect(fullscreenInput).toHaveClass('min-h-0');

    await user.type(fullscreenInput, ' plus more');
    await fireEvent.click(within(fullscreenComposer).getByRole('button', { name: /send/i }));

    await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
      input: 'mobile draft plus more',
      delivery_policy: 'after_idle',
      metadata: { source: 'dashboard_chat' },
    }));
  } finally {
    window.matchMedia = originalMatchMedia;
  }
});


test('shows idle thought summary trigger above the final assistant response', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.timelineState.set({
    sessionId: 'session-2',
    bindingId: 'binding-1',
    items: [
      {
        item_id: 'turn-1:user',
        kind: 'user',
        raw_kind: 'user',
        role: 'user',
        title: null,
        status: null,
        occurred_at: '2026-05-14T00:00:00Z',
        content_preview: 'hello',
        content_ref: 'turn-1:user-ref',
        turn_id: 'turn-1',
      },
      {
        item_id: 'turn-1:thinking',
        kind: 'thinking',
        raw_kind: 'thinking',
        role: 'assistant',
        title: null,
        status: null,
        occurred_at: '2026-05-14T00:00:01Z',
        content_preview: 'I should inspect the code.',
        content_ref: 'turn-1:thinking-ref',
        turn_id: 'turn-1',
      },
      {
        item_id: 'turn-1:tool',
        kind: 'tool_call',
        raw_kind: 'tool_call',
        role: 'tool',
        title: 'read',
        status: 'started',
        occurred_at: '2026-05-14T00:00:02Z',
        content_preview: 'read {"path":"src/app.ts"}',
        content_ref: 'turn-1:tool-ref',
        turn_id: 'turn-1',
      },
      {
        item_id: 'turn-1:assistant',
        kind: 'assistant',
        raw_kind: 'text',
        role: 'assistant',
        title: null,
        status: null,
        occurred_at: '2026-05-14T00:00:03Z',
        content_preview: 'Final answer',
        content_ref: 'turn-1:assistant-ref',
        turn_id: 'turn-1',
      },
    ],
    headCursor: null,
    tailCursor: null,
        sourceId: 'source-1',
    hasMore: false,
        loading: false,
    refreshing: false,
    error: null,
  });
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  const timelineSnapshot = mocks.timelineState.get();
  mocks.loadSessionTimeline.mockImplementationOnce(async () => {
    mocks.timelineState.set(timelineSnapshot);
    return null;
  });

  render(SessionChatPage);

  expect(await screen.findByText('Final answer')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /view thought details/i })).toHaveTextContent('Worked for 2 steps');
  expect(screen.queryByText('Thought for 2 steps')).not.toBeInTheDocument();
  expect(screen.queryByText('I should inspect the code.')).not.toBeInTheDocument();
  expect(screen.queryByText('read {"path":"src/app.ts"}')).not.toBeInTheDocument();
  expect(screen.queryByText('started')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('started')).not.toBeInTheDocument();
});


test('renders assistant output as markdown while leaving user prompts as plain text', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({
      session_id: 'session-2',
      input: { summary: '**literal prompt**' },
      output: { summary: '**bold output**\n\n- first item' },
    })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  const { container } = render(SessionChatPage);

  expect(await screen.findByText('**literal prompt**')).toBeInTheDocument();
  expect(container.querySelector('strong')?.textContent).toBe('bold output');
  expect(container.querySelector('li')?.textContent).toBe('first item');
});


test('highlights fenced code blocks in assistant markdown and copies their text', async () => {
  const writeText = vi.fn(async () => undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({
      session_id: 'session-2',
      output: { summary: '```ts\nconst answer: number = 42;\n```' },
    })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  const { container } = render(SessionChatPage);

  expect(await screen.findByText(/answer/)).toBeInTheDocument();
  expect(container.querySelector('code.hljs.language-ts')).toBeInTheDocument();
  expect(container.querySelector('.hljs-keyword')?.textContent).toBe('const');
  const markdownContainer = Array.from(container.querySelectorAll('div')).find((element) =>
    element.className.includes('[&_pre]:'),
  );
  expect(markdownContainer?.className).not.toContain('[&_pre]:bg-muted');
  expect(markdownContainer?.className).not.toContain('[&_pre]:p-3');
  expect(markdownContainer?.className).not.toContain('[&_pre_code]:bg-transparent');
  expect(markdownContainer?.className).not.toContain('[&_pre_code]:p-0');
  const copyCodeButton = await screen.findByRole('button', { name: /copy code block/i });
  const pre = container.querySelector('pre');
  expect(pre).toHaveClass('w-full');
  expect(pre).toHaveClass('border');
  expect(pre).toHaveClass('border-border');
  const assistantContent = pre?.closest('[data-role="assistant"]')?.firstElementChild;
  expect(assistantContent).toHaveClass('group-[.is-assistant]:w-full');
  expect(assistantContent).not.toHaveClass('group-[.is-assistant]:px-3');
  expect(assistantContent).not.toHaveClass('sm:group-[.is-assistant]:px-4');
  const codeBlockHeader = container.querySelector('[data-code-block-header]');
  expect(codeBlockHeader).toHaveTextContent('ts');
  expect(codeBlockHeader).not.toHaveClass('border-b');
  expect(codeBlockHeader).not.toHaveClass('bg-muted/40');

  expect(navigator.clipboard?.writeText).toBe(writeText);
  expect(copyCodeButton.querySelector('svg')).toBeInTheDocument();
  await fireEvent.click(copyCodeButton);
  await waitFor(() => expect(writeText).toHaveBeenCalledWith('const answer: number = 42;'));
  expect(await screen.findByRole('button', { name: /code block copied/i })).toBeInTheDocument();
});


test('opens an inbox sheet with actionable pending, failed, and dispatching messages only', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [],
    inboxMessages: [
      inboxMessage({
        message_id: 'message-dispatched',
        session_id: 'session-2',
        state: 'dispatched',
        input: { summary: 'Already sent' },
      }),
      inboxMessage({
        message_id: 'message-pending-old',
        session_id: 'session-2',
        state: 'pending',
        input: { summary: 'Continue implementation' },
        updated_at: '2026-05-14T00:00:04Z',
      }),
      inboxMessage({
        message_id: 'message-failed',
        session_id: 'session-2',
        state: 'failed',
        input: { summary: 'Fix the failing dashboard test' },
        metadata: { source: 'dashboard_chat', attempt: 1 },
        delivery_policy: 'interrupt_now',
        turn_id: 'turn-2',
        failure_message: 'runtime unavailable',
        updated_at: '2026-05-14T00:00:05Z',
      }),
      inboxMessage({
        message_id: 'message-dispatching',
        session_id: 'session-2',
        state: 'dispatching',
        input: { summary: 'Sending now' },
        updated_at: '2026-05-14T00:00:06Z',
      }),
    ],
    events: [],
    artifacts: [],
  });

  const { container } = render(SessionChatPage);

  const inboxButton = await screen.findByRole('button', { name: /open inbox, 2 messages/i });
  expect(inboxButton).toHaveTextContent('Inbox');
  expect(inboxButton).toHaveTextContent('2');
  expect(inboxButton).toHaveAttribute('data-chat-desktop-inbox');
  const primaryActions = screen.getByRole('group', { name: /primary session actions/i });
  expect(primaryActions).toHaveClass('flex');
  expect(Array.from(primaryActions.children).map((child) => child.getAttribute('data-slot'))).toEqual(['button', 'button', 'button', 'dropdown-menu-trigger']);
  for (const button of within(primaryActions).getAllByRole('button')) {
    expect(button.className.split(/\s+/)).not.toContain('border');
  }

  const advancedButton = screen.getByRole('button', { name: /advanced session controls, 2 inbox messages/i });
  const advancedBubble = advancedButton.parentElement?.querySelector('[data-chat-mobile-inbox-count]');
  expect(advancedBubble).toHaveTextContent('2');
  expect(advancedBubble).toHaveClass('sm:hidden');

  await fireEvent.click(advancedButton);
  const mobileInboxMenuItem = await screen.findByRole('menuitem', { name: /open inbox, 2 messages/i });
  expect(mobileInboxMenuItem).toHaveClass('sm:hidden');

  await fireEvent.click(mobileInboxMenuItem);

  expect(container.querySelector('button[data-chat-desktop-inbox]')).toBeInTheDocument();

  expect(await screen.findByRole('dialog')).toBeInTheDocument();
  expect(screen.getByText('Sending now')).toBeInTheDocument();
  expect(screen.getByText('Fix the failing dashboard test')).toBeInTheDocument();
  expect(screen.getByText('Continue implementation')).toBeInTheDocument();
  expect(screen.queryByText('Already sent')).not.toBeInTheDocument();
  expect(screen.getByText('runtime unavailable')).toBeInTheDocument();

  const articles = screen.getAllByRole('article');
  expect(articles.map((article) => article.textContent)).toEqual([
    expect.stringContaining('Sending now'),
    expect.stringContaining('Fix the failing dashboard test'),
    expect.stringContaining('Continue implementation'),
  ]);
  expect(articles[0]).not.toHaveTextContent('Cancel');
  expect(articles[0]).not.toHaveTextContent('Retry');
  expect(articles[1]).toHaveTextContent('Retry');
  expect(articles[1]).not.toHaveTextContent('Cancel');
  expect(articles[2]).toHaveTextContent('Cancel');
});


test('supports cancelling pending inbox messages and retrying or removing failed inbox messages', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [],
    inboxMessages: [
      inboxMessage({
        message_id: 'message-pending',
        session_id: 'session-2',
        state: 'pending',
        input: { summary: 'Continue implementation' },
      }),
      inboxMessage({
        message_id: 'message-failed',
        session_id: 'session-2',
        state: 'failed',
        input: { summary: 'Fix the failing dashboard test' },
        metadata: { source: 'dashboard_chat', attempt: 1 },
        delivery_policy: 'interrupt_now',
      }),
    ],
    events: [],
    artifacts: [],
  });

  render(SessionChatPage);
  await userEvent.click(await screen.findByRole('button', { name: /open inbox, 2 messages/i }));

  await userEvent.click(await screen.findByRole('button', { name: /cancel inbox message continue implementation/i }));
  expect(mocks.cancelInboxMessage).toHaveBeenCalledWith('session-2', 'message-pending');

  await userEvent.click(await screen.findByRole('button', { name: /retry inbox message fix the failing dashboard test/i }));
  expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'Fix the failing dashboard test',
    delivery_policy: 'interrupt_now',
    metadata: { source: 'dashboard_chat', attempt: 1 },
  });

  await userEvent.click(await screen.findByRole('button', { name: /remove inbox message fix the failing dashboard test/i }));
  expect(mocks.dismissInboxMessage).toHaveBeenCalledWith('session-2', 'message-failed');
});


test('loads and renders an existing chat session with metadata and workspace name above the prompt input without a page header', async () => {
  const selected = session({
    session_id: 'session-2',
    client_type: 'pi',
    handle: 'second',
    role: 'reviewer',
    description: 'Review dashboard changes',
    execution_profile_id: 'coder',
    execution_profile_version: '1',
    state: 'busy',
    workspace_id: 'workspace-1',
    workspace: '~/repo/pontia',
  });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.workspaces.set([workspace({ workspace_id: 'workspace-1', name: 'pontia', canonical_path: '/repo/pontia', display_path: '~/repo/pontia' })]);

  render(SessionChatPage);

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2'));
  expect(await screen.findByText('hi there')).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /second · reviewer/i })).not.toBeInTheDocument();
  expect(screen.queryByText('Description: Review dashboard changes')).not.toBeInTheDocument();
  const sessionDetailsButton = screen.getByRole('button', { name: /Session details: pontia · pi · coder@1 · second/i });
  await userEvent.click(sessionDetailsButton);
  const clientBadge = screen.getAllByLabelText('Client: pi')[0];
  const profileBadge = screen.getAllByLabelText('Profile: coder@1')[0];
  expect(clientBadge).toBeInTheDocument();
  expect(profileBadge).toBeInTheDocument();
  expect(screen.getAllByLabelText('Handle: second')[0]).toBeInTheDocument();
  expect(within(clientBadge.closest('div') as HTMLElement).getByLabelText('Client')).toHaveClass('lucide-terminal');
  expect(within(profileBadge.closest('div') as HTMLElement).getByLabelText('Profile')).toHaveClass('lucide-bot');
  expect(screen.queryByText('Client: pi')).not.toBeInTheDocument();
  expect(screen.queryByText('Profile: coder@1')).not.toBeInTheDocument();
  expect(screen.queryByText('Handle: second')).not.toBeInTheDocument();
  expect(screen.queryByText('Workspace: workspace-1')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Session state: busy')).not.toBeInTheDocument();
  const workspaceBadge = screen.getAllByLabelText('Workspace: /repo/pontia')[0];
  const workspaceName = within(workspaceBadge).getByText('pontia');
  const followUpInput = screen.getByPlaceholderText('Send a follow-up message…');
  expect(screen.queryByText('State: busy')).not.toBeInTheDocument();
  expect(workspaceBadge).toContainElement(workspaceName);
  expect(sessionDetailsButton.compareDocumentPosition(followUpInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getByRole('button', { name: /new chat/i })).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: /new chat/i })).not.toBeInTheDocument();
});


test('shows supported context usage in chat session metadata while hiding unsupported usage', async () => {
  const withUsage = session({
    session_id: 'session-usage',
    capabilities: { context_usage: 'estimated' },
    context_usage: {
      used_tokens: 42000,
      max_tokens: 128000,
      remaining_tokens: 86000,
      usage_ratio: 0.328125,
      input_tokens: null,
      output_tokens: null,
      cache_tokens: null,
      confidence: 'estimated',
      observed_at: '2026-06-13T00:00:00Z',
    },
    model: 'example-model',
  });
  window.history.pushState({}, '', '/dashboard/chat/session-usage');
  mocks.pathParams = { sessionId: 'session-usage' };
  mocks.loadedSessions = [withUsage];
  mocks.sessions.set([withUsage]);
  mocks.sessionDetail.set({ session: withUsage, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  const contextBadge = await screen.findByRole('button', { name: /Session details: .*33% · 42k \/ 128k/i });
  expect(contextBadge).toBeInTheDocument();
  expect(contextBadge.querySelector('.lucide-gauge')).toBeInTheDocument();
  expect(screen.getAllByText('33% · 42k / 128k')[0]).toBeInTheDocument();
  expect(screen.queryByText('Context 33% · 42k / 128k')).not.toBeInTheDocument();

  cleanup();
  const unsupported = session({ session_id: 'session-unsupported', capabilities: { context_usage: 'unsupported' }, context_usage: null });
  window.history.pushState({}, '', '/dashboard/chat/session-unsupported');
  mocks.pathParams = { sessionId: 'session-unsupported' };
  mocks.loadedSessions = [unsupported];
  mocks.sessions.set([unsupported]);
  mocks.sessionDetail.set({ session: unsupported, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  await screen.findByPlaceholderText('Send a follow-up message…');
  expect(screen.queryByText(/context/i)).not.toBeInTheDocument();
});


test('places session controls near the prompt input and keeps advanced controls in a menu', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  const exitButton = screen.getByRole('button', { name: /exit session/i });
  const advancedButton = screen.getByRole('button', { name: /advanced session controls/i });
  expect(exitButton.compareDocumentPosition(followUpInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(advancedButton.compareDocumentPosition(followUpInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.queryByRole('button', { name: /resume session/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /restart session/i })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /session console/i })).not.toBeInTheDocument();

  await fireEvent.click(advancedButton);
  await fireEvent.click(await screen.findByRole('menuitem', { name: /restart session/i }));
  expect(mocks.restartSession).toHaveBeenCalledWith('session-2');

  await fireEvent.click(advancedButton);
  await fireEvent.click(await screen.findByRole('menuitem', { name: /session console/i }));
  expect(mocks.navigate).toHaveBeenCalledWith('/sessions/session-2');

  await fireEvent.click(exitButton);
  expect(mocks.terminateSession).toHaveBeenCalledWith('session-2');
});


test('disables follow-up input for sessions that do not advertise web-write capability while keeping output visible', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle', capabilities: { accept_task: false, stream_output: true } });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({ session_id: 'session-2', input: { summary: 'tui input' }, output: { summary: 'tui output' } })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  render(SessionChatPage);

  expect(await screen.findByText('tui output')).toBeInTheDocument();
  const followUpInput = screen.getByPlaceholderText('Send a follow-up message…');
  expect(followUpInput).toBeDisabled();
  expect(screen.getByText('此 session 当前不可从 Web 写入')).toBeInTheDocument();

  await user.type(followUpInput, 'should not send');
  await user.click(screen.getByRole('button', { name: /send/i }));

  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
});


test('follow-up composer submits with Enter while preserving modified Enter for newlines', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(SessionChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  expect(followUpInput).toHaveClass('block');
  expect(screen.queryByText('Enter to send · Shift+Enter / Ctrl+Enter for newline')).not.toBeInTheDocument();
  const composerToolbar = screen.getByRole('button', { name: /send/i }).parentElement;
  expect(composerToolbar).toHaveClass('pt-0');
  expect(composerToolbar).not.toHaveClass('pt-2');

  await user.type(followUpInput, 'continue this session');
  expect(await fireEvent.keyDown(followUpInput, { key: 'Enter', shiftKey: true })).toBe(true);
  expect(await fireEvent.keyDown(followUpInput, { key: 'Enter', ctrlKey: true })).toBe(true);
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();

  expect(await fireEvent.keyDown(followUpInput, { key: 'Enter' })).toBe(false);
  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'continue this session',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  }));
});


test('shows a floating scroll-down button away from the bottom and scrolls down when clicked', async () => {
  installIntersectionObserverMock();
  const user = userEvent.setup();
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 2400 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  await screen.findByText('hi there');
  scrollTo.mockClear();
  await triggerLatestBottomIntersection(false);
  const scrollDownButton = await screen.findByRole('button', { name: /scroll to bottom/i });
  const scrollDownContainer = scrollDownButton.closest('[data-chat-scroll-down-container]');
  expect(scrollDownContainer).toHaveClass('transition-[left]');
  expect(scrollDownContainer).toHaveClass('chat-scroll-down-enter');
  expect(scrollDownContainer).toHaveClass('duration-200');
  expect(scrollDownContainer).toHaveClass('ease-linear');
  await user.click(scrollDownButton);

  expect(scrollTo).toHaveBeenCalledWith({ top: 2400 });
  await waitFor(() => expect(screen.queryByRole('button', { name: /scroll to bottom/i })).not.toBeInTheDocument());
  scrollTo.mockRestore();
});


test('shows the floating scroll-down button after switching sessions when the document is away from the bottom', async () => {
  installIntersectionObserverMock();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  const other = session({ session_id: 'session-3', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 2400 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected, other];
  mocks.sessions.set([selected, other]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  await screen.findByText('hi there');
  await triggerLatestBottomIntersection(true);
  expect(screen.queryByRole('button', { name: /scroll to bottom/i })).not.toBeInTheDocument();

  mocks.pathParams = { sessionId: 'session-3' };
  window.history.pushState({}, '', '/dashboard/chat/session-3');
  mocks.sessionDetail.set({ session: other, turns: [turn({ session_id: 'session-3' })], inboxMessages: [], events: [], artifacts: [] });
  window.dispatchEvent(new PopStateEvent('popstate'));
  await waitFor(() => expect(mocks.loadSessionTimeline).toHaveBeenCalledWith('session-3', { mode: 'rebuild' }));
  await triggerLatestBottomIntersection(false);

  expect(await screen.findByRole('button', { name: /scroll to bottom/i })).toBeInTheDocument();
});


test('hides the floating scroll-down button at the bottom', async () => {
  installIntersectionObserverMock();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 2400 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });

  render(SessionChatPage);

  await screen.findByText('hi there');
  await triggerLatestBottomIntersection(true);
  expect(screen.queryByRole('button', { name: /scroll to bottom/i })).not.toBeInTheDocument();
});


test('scrolls to the document bottom after sending from the prompt input', async () => {
  const user = userEvent.setup();
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(SessionChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  await user.type(followUpInput, 'continue this session');
  await user.click(screen.getByRole('button', { name: /send/i }));

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
  scrollTo.mockRestore();
});


test('scrolls again when a prompt input send is rendered after the submit response', async () => {
  const user = userEvent.setup();
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  let scrollHeight = 4096;
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, get: () => scrollHeight });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [], artifacts: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(SessionChatPage);

  await screen.findByText('hi there');
  scrollTo.mockClear();
  const followUpInput = screen.getByPlaceholderText('Send a follow-up message…');
  await user.type(followUpInput, 'continue this session');
  await user.click(screen.getByRole('button', { name: /send/i }));
  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));

  scrollHeight = 5000;
  mocks.timelineState.set({
    ...mocks.timelineState.get(),
    sessionId: 'session-2',
    items: timelineItemsFromTurns([
      turn({ turn_id: 'turn-1', session_id: 'session-2' }),
      turn({ turn_id: 'turn-2', session_id: 'session-2', input: { summary: 'continue this session' }, output: null, completed_at: null }),
    ]),
  });

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 5000 }));
  scrollTo.mockRestore();
});


test('does not scroll to the document bottom after retrying an inbox message', async () => {
  const user = userEvent.setup();
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [],
    inboxMessages: [
      inboxMessage({
        message_id: 'message-failed',
        session_id: 'session-2',
        state: 'failed',
        input: { summary: 'fix the failing dashboard test' },
      }),
    ],
    events: [],
    artifacts: [],
  });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(SessionChatPage);

  await user.click(await screen.findByRole('button', { name: /open inbox, 1 message/i }));
  scrollTo.mockClear();
  await user.click(await screen.findByRole('button', { name: /retry inbox message fix the failing dashboard test/i }));

  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'fix the failing dashboard test',
    delivery_policy: 'after_idle',
    metadata: {},
  }));
  expect(scrollTo).not.toHaveBeenCalled();
  scrollTo.mockRestore();
});


test('does not render inline chat error alerts', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.sessionDetailError.set('Could not load session detail');

  render(SessionChatPage);

  await screen.findByPlaceholderText('Send a follow-up message…');
  await waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith('Chat error', { description: 'Could not load session detail' }));
  expect(screen.queryByText('Chat error')).not.toBeInTheDocument();
  expect(screen.queryByText('Could not load session detail')).not.toBeInTheDocument();
});


test('uses selected session detail state for bottom interrupted status when the session list is stale', async () => {
  const staleListSession = session({ session_id: 'session-2', state: 'busy', current_turn_id: 'turn-1' });
  const interruptedDetailSession = session({ session_id: 'session-2', state: 'interrupted', current_turn_id: null });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [staleListSession];
  mocks.sessions.set([staleListSession]);
  mocks.sessionDetail.set({
    session: interruptedDetailSession,
    turns: [turn({ session_id: 'session-2', state: 'interrupted', output: null, completed_at: '2026-05-14T00:00:03Z' })],
    inboxMessages: [],
    events: [],
    artifacts: [],
  });

  render(SessionChatPage);

  expect(await screen.findByText('session interrupted')).toBeInTheDocument();
  expect(screen.queryByText('Agent working')).not.toBeInTheDocument();
});


test('hides exit on exited sessions and waits for idle after automatic resume before sending a message', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'exited' });
  const starting = session({ session_id: 'session-2', state: 'starting' });
  const idle = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [], artifacts: [] });
  mocks.resumeSession.mockImplementation(async () => {
    mocks.sessions.set([starting]);
    mocks.sessionDetail.set({ session: starting, turns: [], inboxMessages: [], events: [], artifacts: [] });
  });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(SessionChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  expect(followUpInput).not.toBeDisabled();
  expect(screen.queryByRole('button', { name: /exit session/i })).not.toBeInTheDocument();

  await user.type(followUpInput, 'continue this session');
  await user.click(screen.getByRole('button', { name: /send/i }));

  await waitFor(() => expect(mocks.resumeSession).toHaveBeenCalledWith('session-2'));
  expect(followUpInput).toHaveValue('continue this session');
  expect(followUpInput).toBeDisabled();
  expect(screen.getByRole('button', { name: /send/i })).toBeDisabled();
  await Promise.resolve();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();

  mocks.sessions.set([idle]);
  mocks.sessionDetail.set({ session: idle, turns: [], inboxMessages: [], events: [], artifacts: [] });

  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'continue this session',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  }));
  expect(mocks.resumeSession.mock.invocationCallOrder[0]).toBeLessThan(mocks.submitInboxMessage.mock.invocationCallOrder[0]);
});
