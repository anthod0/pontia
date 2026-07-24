import { inboxMessage, mocks, session, timelineItemsFromTurns, timelineStateValue, turn, workspace } from './fixtures';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import type { CreateSessionResult, SessionView, TurnView } from '../../../src/api/types';
import { optimisticInitialMessages, rememberOptimisticMessage } from '../../../src/stores/optimisticChat';

const NewChatPage = (await import('../../../src/pages/NewChatPage.svelte')).default;
const SessionChatPage = (await import('../../../src/pages/SessionChatPage.svelte')).default;

class TestIntersectionObserver implements IntersectionObserver {
  static instances: TestIntersectionObserver[] = [];

  readonly root: Element | Document | null = null;
  readonly rootMargin = '0px';
  readonly thresholds = [0.01];
  observedElement: Element | null = null;

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

function observedHistorySentinels(): Element[] {
  return TestIntersectionObserver.instances
    .map((instance) => instance.observedElement)
    .filter((element): element is Element => Boolean(element?.hasAttribute('data-chat-history-top-sentinel')));
}

function prepareBranchChat(
  turns: TurnView[],
  sessionOverrides: Partial<SessionView> = {},
): SessionView {
  const selected = session({
    session_id: 'session-branch',
    state: 'idle',
    capabilities: { accept_task: true, timeline: true, topology: true, branch_control: true },
    ...sessionOverrides,
  });
  window.history.pushState({}, '', '/dashboard/chat/session-branch');
  mocks.pathParams = { sessionId: 'session-branch' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: turns.map((item) => ({ ...item, session_id: 'session-branch' })),
    inboxMessages: [],
    events: [],
  });
  return selected;
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
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });

  render(SessionChatPage);

  await fireEvent.click(await screen.findByRole('button', { name: /advanced session controls/i }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: /new chat/i }));

  expect(mocks.navigate).toHaveBeenCalledWith('/chat', { workspace: 'workspace-2' });
});


test('replaces Send with Interrupt in the empty composer for a busy interruptible session', async () => {
  const busySession = session({ state: 'busy', current_turn_id: 'turn-1', capabilities: { interrupt: true, timeline: true } });
  mocks.loadedSessions = [busySession];
  mocks.sessions.set([busySession]);
  mocks.sessionDetail.set({ session: busySession, turns: [turn({ state: 'running', output: null, completed_at: null })], inboxMessages: [], events: [] });
  mocks.pathParams = { sessionId: 'session-1' };
  window.history.pushState({}, '', '/dashboard/chat/session-1');

  render(SessionChatPage);

  const agentStatus = await screen.findByLabelText('Agent status: Agent working');
  expect(within(agentStatus).queryByRole('button', { name: /interrupt agent/i })).not.toBeInTheDocument();
  const composer = document.querySelector('[data-chat-composer-dock="fixed"]');
  expect(composer).toBeInTheDocument();
  const interruptButton = within(composer as HTMLElement).getByRole('button', { name: /interrupt agent/i });
  expect(within(composer as HTMLElement).queryByRole('button', { name: /^send$/i })).not.toBeInTheDocument();

  await fireEvent.click(interruptButton);
  await waitFor(() => expect(mocks.interruptSession).toHaveBeenCalledWith('session-1'));
});

test.each([
  ['idle session', { state: 'idle', capabilities: { interrupt: true, timeline: true } }],
  ['runtime without interrupt capability', { state: 'busy', capabilities: { interrupt: false, timeline: true } }],
] as const)('keeps Send for an empty composer when the %s is not interruptible', async (_label, overrides) => {
  const selected = session(overrides);
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
  mocks.pathParams = { sessionId: 'session-1' };
  window.history.pushState({}, '', '/dashboard/chat/session-1');

  render(SessionChatPage);

  await screen.findByPlaceholderText('Send a follow-up message…');
  expect(screen.queryByRole('button', { name: /interrupt agent/i })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: /^send$/i })).toBeInTheDocument();
});

test('keeps Send and queues inbox input while an interruptible session is busy', async () => {
  const busySession = session({
    state: 'busy',
    current_turn_id: 'turn-1',
    capabilities: { accept_task: true, interrupt: true, timeline: true },
  });
  mocks.loadedSessions = [busySession];
  mocks.sessions.set([busySession]);
  mocks.sessionDetail.set({ session: busySession, turns: [turn({ state: 'running', output: null, completed_at: null })], inboxMessages: [], events: [] });
  mocks.pathParams = { sessionId: 'session-1' };
  window.history.pushState({}, '', '/dashboard/chat/session-1');

  render(SessionChatPage);

  const input = await screen.findByPlaceholderText('Send a follow-up message…');
  await userEvent.type(input, 'Queue this follow-up');

  expect(screen.queryByRole('button', { name: /interrupt agent/i })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: /^send$/i }));
  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-1', {
    input: 'Queue this follow-up',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  }));
  expect(mocks.interruptSession).not.toHaveBeenCalled();
});


test('selects tree timeline loading when the Session advertises timeline and topology', async () => {
  const selected = session({
    session_id: 'session-tree',
    current_turn_id: 'turn-5',
    capabilities: { timeline: true, topology: true },
  });
  window.history.pushState({}, '', '/dashboard/chat/session-tree');
  mocks.pathParams = { sessionId: 'session-tree' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({ turn_id: 'turn-5', session_id: 'session-tree' })],
    inboxMessages: [],
    events: [],
  });

  render(SessionChatPage);

  await waitFor(() => expect(mocks.loadSessionTimeline).toHaveBeenCalledWith('session-tree', {
    mode: 'rebuild',
    latestTurnId: 'turn-5',
    topology: true,
  }));
});

test('offers local Edit and Resend controls on eligible projected user messages', async () => {
  const user = userEvent.setup();
  const originalTurn = turn({
    turn_id: 'turn-original',
    input: { summary: 'Inspect the original implementation.' },
    output: { summary: 'The original implementation is ready.' },
  });
  prepareBranchChat([originalTurn], { current_turn_id: 'turn-original' });
  rememberOptimisticMessage('session-branch', 'Optimistic follow-up');
  optimisticInitialMessages.update((messages) => ({
    ...messages,
    'session-branch': [
      ...(messages['session-branch'] ?? []),
      {
        id: 'failed-placeholder:user',
        turnId: 'failed-placeholder',
        role: 'user',
        content: 'Failed follow-up',
        status: 'failed',
        createdAt: '2026-05-14T00:02:00Z',
      },
    ],
  }));

  render(SessionChatPage);

  const editButton = await screen.findByRole('button', { name: 'Edit message: Inspect the original implementation.' });
  expect(screen.getByRole('button', { name: 'Resend message: Inspect the original implementation.' })).toBeInTheDocument();
  expect(screen.getAllByRole('button', { name: /^Edit message:/ })).toHaveLength(1);
  expect(screen.getAllByRole('button', { name: /^Resend message:/ })).toHaveLength(1);
  expect(screen.getByText('Optimistic follow-up')).toBeInTheDocument();
  expect(screen.getByText('Failed follow-up')).toBeInTheDocument();

  await user.click(editButton);

  expect(screen.getByRole('textbox', { name: 'Edit historical message' })).toHaveValue('Inspect the original implementation.');
  expect(screen.getByText(/does not rewind workspace files or external side effects/i)).toBeInTheDocument();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();

  await fireEvent.keyDown(screen.getByRole('textbox', { name: 'Edit historical message' }), { key: 'Escape' });
  expect(screen.queryByRole('textbox', { name: 'Edit historical message' })).not.toBeInTheDocument();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();

  await user.click(screen.getByRole('button', { name: 'Edit message: Inspect the original implementation.' }));
  await user.click(screen.getByRole('button', { name: 'Cancel editing' }));

  expect(screen.queryByRole('textbox', { name: 'Edit historical message' })).not.toBeInTheDocument();
  expect(screen.getByText('Inspect the original implementation.')).toBeInTheDocument();
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();
});

test('offers branch actions only on the primary user message represented by a projected Turn', async () => {
  const completeHistoricalInput = '  Primary user input with the complete historical text  ';
  prepareBranchChat([turn({
    turn_id: 'turn-original',
    input: { summary: 'Primary user input with the complete…' },
    output: { summary: 'Assistant output' },
  })]);
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    mocks.timelineState.set(timelineStateValue({
      sessionId,
      mode: 'tree',
      status: 'ready',
      latestTurnId: 'turn-original',
      items: [
        {
          item_id: 'turn-original:user-primary',
          kind: 'user',
          role: 'user',
          title: null,
          status: null,
          occurred_at: '2026-05-14T00:00:00Z',
          content_preview: completeHistoricalInput,
          turn_id: 'turn-original',
        },
        {
          item_id: 'turn-original:user-secondary',
          kind: 'user',
          role: 'user',
          title: null,
          status: null,
          occurred_at: '2026-05-14T00:00:01Z',
          content_preview: 'Secondary user activity',
          turn_id: 'turn-original',
        },
        {
          item_id: 'turn-original:assistant',
          kind: 'assistant',
          role: 'assistant',
          title: null,
          status: null,
          occurred_at: '2026-05-14T00:00:02Z',
          content_preview: 'Assistant output',
          turn_id: 'turn-original',
        },
      ],
    }));
    return null;
  });

  render(SessionChatPage);

  const editButton = await screen.findByRole('button', { name: /Edit message: Primary user input with the complete historical text/ });
  expect(screen.queryByRole('button', { name: 'Edit message: Secondary user activity' })).not.toBeInTheDocument();
  expect(screen.getAllByRole('button', { name: /^Edit message:/ })).toHaveLength(1);

  await userEvent.click(editButton);
  expect(screen.getByRole('textbox', { name: 'Edit historical message' }))
    .toHaveValue(completeHistoricalInput);
  await userEvent.click(screen.getByRole('button', { name: 'Cancel editing' }));
  await userEvent.click(screen.getByRole('button', { name: /^Resend message: Primary user input/ }));
  expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-branch', {
    input: completeHistoricalInput,
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat_branch_resend' },
    branch_target_turn_id: 'turn-original',
  });
});

test('disables all branch actions when any projected Turn is active', async () => {
  prepareBranchChat([
    turn({ turn_id: 'turn-completed', input: { summary: 'Completed historical input' } }),
    turn({
      turn_id: 'turn-active',
      parent_turn_id: 'turn-completed',
      state: 'running',
      input: { summary: 'Active input' },
      output: null,
      completed_at: null,
      created_at: '2026-05-14T00:01:00Z',
    }),
  ]);

  render(SessionChatPage);

  await screen.findByText('Completed historical input');
  expect(screen.queryByRole('button', { name: /^Edit message:/ })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /^Resend message:/ })).not.toBeInTheDocument();
});

test.each([
  {
    name: 'branch control is unsupported',
    sessionOverrides: { capabilities: { accept_task: true, timeline: true, topology: true, branch_control: false } },
    turnOverrides: {},
  },
  {
    name: 'the Session is busy',
    sessionOverrides: { state: 'busy', current_turn_id: 'turn-original' },
    turnOverrides: { state: 'running', completed_at: null },
  },
  {
    name: 'the projected Turn is active',
    sessionOverrides: {},
    turnOverrides: { state: 'running', completed_at: null },
  },
  {
    name: 'the projected user message failed',
    sessionOverrides: {},
    turnOverrides: { failure: { message: 'Input was rejected' }, input: null },
  },
])('does not expose branch actions when $name', async ({ sessionOverrides, turnOverrides }) => {
  prepareBranchChat([
    turn({
      turn_id: 'turn-original',
      input: { summary: 'Historical input' },
      output: { summary: 'Historical output' },
      ...turnOverrides,
    }),
  ], sessionOverrides);

  render(SessionChatPage);

  await screen.findByText(turnOverrides.input === null ? 'No input was reported.' : 'Historical input');
  expect(screen.queryByRole('button', { name: /^Edit message:/ })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /^Resend message:/ })).not.toBeInTheDocument();
});

test('submits Edit and Resend through the shared branch-targeted Inbox mutation without changing the visible suffix', async () => {
  const user = userEvent.setup();
  const originalTurn = turn({
    turn_id: 'turn-original',
    input: { summary: 'Original question' },
    output: { summary: 'Original answer' },
  });
  const suffixTurn = turn({
    turn_id: 'turn-suffix',
    parent_turn_id: 'turn-original',
    input: { summary: 'Current suffix question' },
    output: { summary: 'Current suffix answer' },
    created_at: '2026-05-14T00:01:00Z',
    started_at: '2026-05-14T00:01:01Z',
    completed_at: '2026-05-14T00:01:02Z',
  });
  prepareBranchChat([originalTurn, suffixTurn]);
  mocks.submitInboxMessage.mockResolvedValue(inboxMessage({
    message_id: 'message-edit',
    session_id: 'session-branch',
    input: { summary: 'Corrected question' },
    branch_target_turn_id: 'turn-original',
  }));

  render(SessionChatPage);

  await user.click(await screen.findByRole('button', { name: 'Edit message: Original question' }));
  const editor = screen.getByRole('textbox', { name: 'Edit historical message' });
  await user.clear(editor);
  await user.type(editor, 'Corrected question');
  await fireEvent.keyDown(editor, { key: 'Enter', ctrlKey: true });

  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-branch', {
    input: 'Corrected question',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat_branch_edit' },
    branch_target_turn_id: 'turn-original',
  }));
  expect(screen.queryByRole('textbox', { name: 'Edit historical message' })).not.toBeInTheDocument();
  expect(screen.getByText('Original question')).toBeInTheDocument();
  expect(screen.getByText('Current suffix question')).toBeInTheDocument();
  expect(screen.queryByText('Corrected question')).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Resend message: Original question' }));

  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenLastCalledWith('session-branch', {
    input: 'Original question',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat_branch_resend' },
    branch_target_turn_id: 'turn-original',
  }));
  expect(screen.getByText('Current suffix question')).toBeInTheDocument();
});

test('rejects a blank edit locally and disables competing branch actions while submitting', async () => {
  const user = userEvent.setup();
  const originalTurn = turn({ turn_id: 'turn-original', input: { summary: 'Original question' } });
  const otherTurn = turn({
    turn_id: 'turn-other',
    parent_turn_id: 'turn-original',
    input: { summary: 'Other question' },
    created_at: '2026-05-14T00:01:00Z',
  });
  prepareBranchChat([originalTurn, otherTurn]);
  let resolveSubmission: (() => void) | null = null;
  mocks.submitInboxMessage.mockImplementation(() => new Promise((resolve) => {
    resolveSubmission = () => resolve(inboxMessage({
      session_id: 'session-branch',
      input: { summary: 'Original question' },
      branch_target_turn_id: 'turn-original',
    }));
  }));

  render(SessionChatPage);

  await user.click(await screen.findByRole('button', { name: 'Edit message: Original question' }));
  const editor = screen.getByRole('textbox', { name: 'Edit historical message' });
  await user.clear(editor);
  await user.type(editor, '   ');
  expect(screen.getByRole('button', { name: 'Send edit' })).toBeDisabled();
  await fireEvent.keyDown(editor, { key: 'Enter', ctrlKey: true });
  expect(mocks.submitInboxMessage).not.toHaveBeenCalled();

  await user.click(screen.getByRole('button', { name: 'Cancel editing' }));
  await user.click(screen.getByRole('button', { name: 'Resend message: Original question' }));

  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledTimes(1));
  expect(screen.getByRole('button', { name: 'Edit message: Original question' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Resend message: Other question' })).toBeDisabled();
  resolveSubmission?.();
  await waitFor(() => expect(screen.getByRole('button', { name: 'Edit message: Original question' })).toBeEnabled());
});

test('presents branch submission errors and keeps editing available for correction', async () => {
  const user = userEvent.setup();
  prepareBranchChat([turn({ turn_id: 'turn-original', input: { summary: 'Original question' } })]);
  mocks.submitInboxMessage.mockRejectedValue(new Error('Target Turn can no longer be resolved'));

  render(SessionChatPage);

  await user.click(await screen.findByRole('button', { name: 'Edit message: Original question' }));
  await user.click(screen.getByRole('button', { name: 'Send edit' }));

  expect(await screen.findByRole('alert')).toHaveTextContent('Target Turn can no longer be resolved');
  expect(screen.getByRole('textbox', { name: 'Edit historical message' })).toHaveValue('Original question');
  expect(screen.getByRole('button', { name: 'Send edit' })).toBeEnabled();
});

test('keeps the divergent suffix until a projected tree update replaces it', async () => {
  const user = userEvent.setup();
  const rootTurn = turn({ turn_id: 'turn-root', input: { summary: 'Root question' }, output: { summary: 'Root answer' } });
  const originalTurn = turn({
    turn_id: 'turn-original',
    parent_turn_id: 'turn-root',
    input: { summary: 'Original middle question' },
    output: { summary: 'Original middle answer' },
    created_at: '2026-05-14T00:01:00Z',
  });
  const oldSuffix = turn({
    turn_id: 'turn-old-suffix',
    parent_turn_id: 'turn-original',
    input: { summary: 'Abandoned suffix question' },
    output: { summary: 'Abandoned suffix answer' },
    created_at: '2026-05-14T00:02:00Z',
  });
  prepareBranchChat([rootTurn, originalTurn, oldSuffix]);
  mocks.submitInboxMessage.mockResolvedValue(inboxMessage({
    session_id: 'session-branch',
    input: { summary: 'Replacement middle question' },
    branch_target_turn_id: 'turn-original',
  }));

  render(SessionChatPage);

  await user.click(await screen.findByRole('button', { name: 'Resend message: Original middle question' }));
  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalled());
  expect(screen.getByText('Abandoned suffix question')).toBeInTheDocument();

  const replacementTurn = turn({
    turn_id: 'turn-replacement',
    session_id: 'session-branch',
    parent_turn_id: 'turn-root',
    input: { summary: 'Replacement middle question' },
    output: { summary: 'Replacement middle answer' },
    created_at: '2026-05-14T00:03:00Z',
  });
  mocks.timelineState.set(timelineStateValue({
    sessionId: 'session-branch',
    mode: 'tree',
    groups: [],
    items: timelineItemsFromTurns([rootTurn, replacementTurn]),
    latestTurnId: 'turn-replacement',
    status: 'ready',
  }));

  expect(await screen.findByText('Replacement middle question')).toBeInTheDocument();
  expect(screen.queryByText('Abandoned suffix question')).not.toBeInTheDocument();
});


test('renames the selected chat session from advanced controls', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', title: 'Old title' });
  const renamed = session({ session_id: 'session-2', title: 'New title' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
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
    mocks.sessionDetail.set({ session: created, turns: [initialTurn], inboxMessages: [], events: [] });
    return { session: created, initial_turn: initialTurn } satisfies CreateSessionResult;
  });
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    mocks.timelineState.set(timelineStateValue({ sessionId, status: 'empty' }));
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


test('does not substitute projected Turn summaries while timeline history is still loading', async () => {
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
  mocks.sessionDetail.set({ session: selected, turns: [activeTurn], inboxMessages: [], events: [] });
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    mocks.timelineState.set(timelineStateValue({ sessionId, loading: true, status: 'loading' }));
    return null;
  });

  render(SessionChatPage);

  expect(await screen.findByText('Loading conversation…')).toBeInTheDocument();
  expect(screen.queryByText('typed in tui')).not.toBeInTheDocument();
});


test('shows workspace git status in the selected chat composer summary', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle', workspace_id: 'workspace-1', workspace: '/repo/pontia', handle: null });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
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


test('does not show the empty conversation state while the selected chat is initializing', async () => {
  let resolveSessions: (() => void) | null = null;
  const selected = session({ session_id: 'session-2', state: 'idle', capabilities: { timeline: true } });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  mocks.loadSessions.mockImplementationOnce(async () => {
    await new Promise<void>((resolve) => (resolveSessions = resolve));
    return [selected];
  });

  try {
    render(SessionChatPage);

    await screen.findByRole('button', { name: /advanced session controls/i });
    expect(screen.queryByText('No messages yet')).not.toBeInTheDocument();
    expect(document.querySelector('[data-chat-conversation-skeleton]')).toBeInTheDocument();
  } finally {
    resolveSessions?.();
  }
});


test('keeps the selected chat transcript hidden until the initial bottom scroll settles', async () => {
  let resolveTimeline: (() => void) | null = null;
  const selected = session({ session_id: 'session-2', state: 'idle', capabilities: { timeline: true } });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    await new Promise<void>((resolve) => (resolveTimeline = resolve));
    mocks.timelineState.set(timelineStateValue({
      sessionId,
      items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
      latestTurnId: 'turn-1',
      status: 'ready',
    }));
    return null;
  });

  render(SessionChatPage);

  await waitFor(() => expect(mocks.loadSessionTimeline).toHaveBeenCalledWith('session-2', { mode: 'rebuild', latestTurnId: 'turn-1' }));
  expect(document.querySelector('[data-chat-initial-scroll-pending="true"]')).toBeInTheDocument();

  resolveTimeline?.();
  await waitFor(() => expect(document.querySelector('[data-chat-initial-scroll-pending="true"]')).not.toBeInTheDocument());
});


test('scrolls to the document bottom after entering a selected chat', async () => {
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });

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
    mocks.sessionDetail.set({ session: firstSession, turns: [turn({ session_id: 'session-1' })], inboxMessages: [], events: [] });
    mocks.loadSessionDetail.mockImplementation(async (sessionId: string) => {
      const selected = sessionId === 'session-2' ? secondSession : firstSession;
      mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: sessionId })], inboxMessages: [], events: [] });
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
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  mocks.timelineState.set(timelineStateValue({
    sessionId: 'session-2',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    nextOlderTurnId: 'turn-older',
    latestTurnId: 'turn-1',
    hasMore: true,
    status: 'ready',
  }));

  render(SessionChatPage);

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
  Object.defineProperty(window, 'scrollY', { configurable: true, value: 40 });
  window.dispatchEvent(new Event('scroll'));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalledWith('session-2', { mode: 'more' });
  scrollTo.mockRestore();
});


test('loads earlier chat history only after pulling beyond the top history sentinel', async () => {
  installIntersectionObserverMock();
  const selected = session({ session_id: 'session-2', state: 'idle', capabilities: { timeline: true } });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  mocks.timelineState.set(timelineStateValue({
    sessionId: 'session-2',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    nextOlderTurnId: 'turn-older',
    latestTurnId: 'turn-1',
    hasMore: true,
    status: 'ready',
  }));

  render(SessionChatPage);

  expect(screen.queryByRole('button', { name: /load earlier messages/i })).not.toBeInTheDocument();

  await waitFor(() => expect(observedHistorySentinels()).toHaveLength(1));
  TestIntersectionObserver.instances.find((instance) => instance.observedElement?.hasAttribute('data-chat-history-top-sentinel'))?.trigger(true);
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalledWith('session-2', { mode: 'more' });

  window.dispatchEvent(new WheelEvent('wheel', { deltaY: -120 }));

  await waitFor(() => expect(mocks.loadSessionTimeline).toHaveBeenCalledWith('session-2', { mode: 'more' }));
});


test('refreshes an already-loaded selected chat through its latest projected Turn without rebuilding history', async () => {
  const selected = session({ session_id: 'session-2', state: 'running' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  mocks.timelineState.set(timelineStateValue({
    sessionId: 'session-2',
    items: timelineItemsFromTurns([
      turn({ turn_id: 'turn-older', session_id: 'session-2', input: { summary: 'older question' }, output: { summary: 'older answer' } }),
      turn({ turn_id: 'turn-latest', session_id: 'session-2', input: { summary: 'latest question' }, output: { summary: 'latest answer' } }),
    ]),
    nextOlderTurnId: 'turn-older',
    latestTurnId: 'turn-latest',
    hasMore: true,
    status: 'ready',
  }));

  render(SessionChatPage);

  await waitFor(() => expect(mocks.refreshSessionTimeline).toHaveBeenCalledWith('session-2', 'turn-1'));
  expect(mocks.loadSessionTimeline).not.toHaveBeenCalledWith('session-2', { mode: 'rebuild' });
  expect(mocks.resetTimelineState).not.toHaveBeenCalledWith('session-2');
  await waitFor(() => expect(mocks.dashboardEventListeners.size).toBe(1));
  mocks.loadSessionDetail.mockClear();
  mocks.loadSessionTimeline.mockClear();
  mocks.refreshSessionTimeline.mockClear();
  await new Promise((resolve) => setTimeout(resolve, 0));
  mocks.loadSessionTimeline.mockClear();
  mocks.refreshSessionTimeline.mockClear();
  mocks.timelineState.set(timelineStateValue({
    ...mocks.timelineState.get(),
    sessionId: 'session-2',
    items: timelineItemsFromTurns([
      turn({ turn_id: 'turn-older', session_id: 'session-2', input: { summary: 'older question' }, output: { summary: 'older answer' } }),
      turn({ turn_id: 'turn-latest', session_id: 'session-2', input: { summary: 'latest question' }, output: { summary: 'latest answer' } }),
    ]),
  }));

  window.dispatchEvent(new Event('focus'));

  await waitFor(() => expect(mocks.loadSessionDetail).toHaveBeenCalledWith('session-2', { showLoading: false }));
  expect(mocks.refreshSessionTimeline).toHaveBeenCalledWith('session-2', 'turn-1');
});


test('coalesces bursty selected-session idle events into one git status refresh', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle', workspace_id: 'workspace-1' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });

  render(SessionChatPage);

  await waitFor(() => expect(mocks.dashboardEventListeners.size).toBe(1));
  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1'));
  mocks.refreshWorkspaceGitStatus.mockClear();
  mocks.refreshSessionTimeline.mockClear();

  const idleEvent = (eventId: string, type: string, turnId: string | null = null) => ({
    kind: 'session_event' as const,
    id: eventId,
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: eventId,
      session_id: 'session-2',
      turn_id: turnId,
      source: 'runtime',
      type,
      time: '2026-05-14T00:00:00Z',
      payload: {},
    },
  });

  for (const listener of mocks.dashboardEventListeners) {
    listener(idleEvent('evt-ready', 'session.ready'));
    listener(idleEvent('evt-completed', 'turn.completed', 'turn-1'));
    listener(idleEvent('evt-failed', 'turn.failed'));
  }

  await waitFor(() => expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledTimes(1));
  expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith('workspace-1');
  expect(mocks.refreshSessionTimeline).toHaveBeenCalledWith('session-2', 'turn-1');
});


test('does not toast transient network errors from automatic chat refreshes', async () => {
  const selected = session({ session_id: 'session-2', state: 'running' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  mocks.timelineState.set(timelineStateValue({
    sessionId: 'session-2',
    items: timelineItemsFromTurns([turn({ session_id: 'session-2' })]),
    latestTurnId: 'turn-1',
    status: 'ready',
  }));

  render(SessionChatPage);

  await waitFor(() => expect(mocks.refreshSessionTimeline).toHaveBeenCalledWith('session-2', 'turn-1'));
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
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
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
  mocks.timelineState.set(timelineStateValue({
    sessionId: 'session-2',
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
        turn_id: 'turn-1',
      },
    ],
    latestTurnId: 'turn-1',
    status: 'ready',
  }));
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
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

test('retries a failed branch delivery with its original target', async () => {
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
        message_id: 'message-branch-failed',
        session_id: 'session-2',
        state: 'failed',
        input: { summary: 'Corrected historical input' },
        metadata: { source: 'dashboard_chat_branch_edit' },
        branch_target_turn_id: 'turn-original',
        failure_message: 'Pi navigation failed',
      }),
    ],
    events: [],
  });

  render(SessionChatPage);
  await userEvent.click(await screen.findByRole('button', { name: /open inbox, 1 message/i }));

  expect(screen.getByText('Pi navigation failed')).toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: /retry inbox message corrected historical input/i }));

  expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'Corrected historical input',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat_branch_edit' },
    branch_target_turn_id: 'turn-original',
  });
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
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
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
  mocks.sessionDetail.set({ session: withUsage, turns: [], inboxMessages: [], events: [] });

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
  mocks.sessionDetail.set({ session: unsupported, turns: [], inboxMessages: [], events: [] });

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
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });

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
  const selected = session({ session_id: 'session-2', state: 'idle', capabilities: { accept_task: false, stream_output: true, timeline: true } });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({
    session: selected,
    turns: [turn({ session_id: 'session-2', input: { summary: 'tui input' }, output: { summary: 'tui output' } })],
    inboxMessages: [],
    events: [],
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


test('shows delivery loading on the submit button instead of below the optimistic message', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-optimistic', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-optimistic');
  mocks.pathParams = { sessionId: 'session-optimistic' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
  let resolveSubmission: (() => void) | null = null;
  mocks.submitInboxMessage.mockImplementation(() => new Promise((resolve) => {
    resolveSubmission = () => resolve(inboxMessage({
      message_id: 'message-optimistic',
      session_id: 'session-optimistic',
      input: { summary: 'slow network message' },
    }));
  }));

  render(SessionChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  await user.type(followUpInput, 'slow network message');
  await user.click(screen.getByRole('button', { name: /send/i }));

  const optimisticMessage = await screen.findByText('slow network message');
  expect(optimisticMessage.closest('[data-role="user"]')).toBeInTheDocument();
  expect(screen.queryByLabelText('Message delivery pending')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Sending message' })).toHaveAttribute('aria-busy', 'true');

  mocks.timelineState.set(timelineStateValue({
    ...mocks.timelineState.get(),
    sessionId: 'session-optimistic',
    items: timelineItemsFromTurns([turn({
      turn_id: 'turn-optimistic',
      session_id: 'session-optimistic',
      input: { summary: 'slow network message' },
      output: null,
      state: 'running',
      completed_at: null,
    })]),
  }));

  expect(screen.getByRole('button', { name: 'Sending message' })).toBeInTheDocument();
  expect(screen.getAllByText('slow network message')).toHaveLength(1);
  resolveSubmission?.();
  await waitFor(() => expect(screen.queryByRole('button', { name: 'Sending message' })).not.toBeInTheDocument());
});


test('follow-up composer submits with Enter while preserving modified Enter for newlines', async () => {
  const user = userEvent.setup();
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
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


test('enables history intersection loading only after initial timeline load and bottom scroll', async () => {
  installIntersectionObserverMock();
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 2400 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  let resolveTimeline: () => void = () => {};
  const timelineLoaded = new Promise<void>((resolve) => {
    resolveTimeline = resolve;
  });
  mocks.loadSessionTimeline.mockImplementationOnce(async (sessionId: string) => {
    await timelineLoaded;
    mocks.timelineState.set(timelineStateValue({
      sessionId,
      items: timelineItemsFromTurns([turn({ session_id: sessionId })]),
      nextOlderTurnId: 'turn-older',
      latestTurnId: 'turn-1',
      hasMore: true,
      status: 'ready',
    }));
    return null;
  });

  render(SessionChatPage);

  await waitFor(() => expect(mocks.loadSessionTimeline).toHaveBeenCalledWith('session-2', { mode: 'rebuild', latestTurnId: 'turn-1' }));
  expect(observedHistorySentinels()).toHaveLength(0);

  resolveTimeline();

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 2400 }));
  await waitFor(() => expect(observedHistorySentinels()).toHaveLength(1));
  scrollTo.mockRestore();
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
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });

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
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });

  render(SessionChatPage);

  await screen.findByText('hi there');
  await triggerLatestBottomIntersection(true);
  expect(screen.queryByRole('button', { name: /scroll to bottom/i })).not.toBeInTheDocument();

  mocks.pathParams = { sessionId: 'session-3' };
  window.history.pushState({}, '', '/dashboard/chat/session-3');
  mocks.sessionDetail.set({ session: other, turns: [turn({ session_id: 'session-3' })], inboxMessages: [], events: [] });
  window.dispatchEvent(new PopStateEvent('popstate'));
  await waitFor(() => expect(mocks.loadSessionTimeline).toHaveBeenCalledWith('session-3', { mode: 'rebuild', latestTurnId: 'turn-1' }));
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
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });

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
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(SessionChatPage);

  const followUpInput = await screen.findByPlaceholderText('Send a follow-up message…');
  await user.type(followUpInput, 'continue this session');
  await user.click(screen.getByRole('button', { name: /send/i }));

  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));
  scrollTo.mockRestore();
});


test('scrolls when a prompt input send is rendered in an existing projected timeline', async () => {
  const user = userEvent.setup();
  const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  const selected = session({ session_id: 'session-2', state: 'idle' });
  Object.defineProperty(document.documentElement, 'scrollHeight', { configurable: true, value: 4096 });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  mocks.submitInboxMessage.mockResolvedValue(undefined);

  render(SessionChatPage);

  await screen.findByText('hi there');
  scrollTo.mockClear();
  const followUpInput = screen.getByPlaceholderText('Send a follow-up message…');
  await user.type(followUpInput, 'continue this session');
  await user.click(screen.getByRole('button', { name: /send/i }));
  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 4096 }));

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


test('does not toast passive chat errors', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle' });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
  mocks.sessionDetailError.set('Could not load session detail');

  render(SessionChatPage);

  await screen.findByPlaceholderText('Send a follow-up message…');
  expect(mocks.toastError).not.toHaveBeenCalled();
});

test('renders an explicit degraded state instead of projected or partial history when timeline loading fails', async () => {
  const selected = session({ session_id: 'session-2', state: 'idle', capabilities: { timeline: true } });
  window.history.pushState({}, '', '/dashboard/chat/session-2');
  mocks.pathParams = { sessionId: 'session-2' };
  mocks.loadedSessions = [selected];
  mocks.sessions.set([selected]);
  mocks.sessionDetail.set({ session: selected, turns: [turn({ session_id: 'session-2' })], inboxMessages: [], events: [] });
  mocks.loadSessionTimeline.mockImplementation(async (sessionId: string) => {
    mocks.timelineState.set(timelineStateValue({
      sessionId,
      items: [],
      nextOlderTurnId: null,
      latestTurnId: 'turn-1',
      hasMore: false,
      loading: false,
      refreshing: false,
      refreshKind: null,
      status: 'range_invalid',
      errorCode: 'turn_timeline_invalid',
      error: 'Turn turn-1 has an invalid timeline range',
    }));
    return null;
  });

  render(SessionChatPage);

  expect(await screen.findByText('Conversation history unavailable')).toBeInTheDocument();
  expect(screen.getByText('Turn turn-1 has an invalid timeline range')).toBeInTheDocument();
  expect(mocks.toastError).not.toHaveBeenCalled();
  expect(screen.queryByText('hi there')).not.toBeInTheDocument();
  expect(document.querySelector('[data-timeline-status="range_invalid"]')).toBeInTheDocument();
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
  mocks.sessionDetail.set({ session: selected, turns: [], inboxMessages: [], events: [] });
  mocks.resumeSession.mockImplementation(async () => {
    mocks.sessions.set([starting]);
    mocks.sessionDetail.set({ session: starting, turns: [], inboxMessages: [], events: [] });
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
  mocks.sessionDetail.set({ session: idle, turns: [], inboxMessages: [], events: [] });

  await waitFor(() => expect(mocks.submitInboxMessage).toHaveBeenCalledWith('session-2', {
    input: 'continue this session',
    delivery_policy: 'after_idle',
    metadata: { source: 'dashboard_chat' },
  }));
  expect(mocks.resumeSession.mock.invocationCallOrder[0]).toBeLessThan(mocks.submitInboxMessage.mock.invocationCallOrder[0]);
});
