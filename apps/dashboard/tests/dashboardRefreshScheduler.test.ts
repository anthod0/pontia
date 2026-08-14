import { expect, test } from 'vitest';
import { createDashboardRefreshScheduler } from '../src/services/dashboardRefreshScheduler.ts';
import type { DashboardStreamEvent } from '../src/api/types.ts';

function taskEvent(taskId: string): DashboardStreamEvent {
  return {
    kind: 'task_event', id: `event-${taskId}`, occurred_at: '2026-05-14T00:00:00Z',
    event: { event_id: `event-${taskId}`, task_id: taskId, event_type: 'task.updated', payload: {}, created_at: '2026-05-14T00:00:00Z' },
  };
}

function sessionEvent(type = 'session.updated', sessionId = 'session-1'): DashboardStreamEvent {
  return {
    kind: 'session_event', id: `event-session-${type}`, occurred_at: '2026-05-14T00:00:00Z',
    event: { event_id: `event-session-${type}`, session_id: sessionId, turn_id: null, source: 'runtime', type, time: '2026-05-14T00:00:00Z', payload: {} },
  };
}

function scheduler(calls: string[], options: {
  taskId?: string | null;
  sessionId?: string | null;
  workflowId?: string | null;
  workflowSessionIds?: string[];
} = {}) {
  return createDashboardRefreshScheduler({
    delayMs: 0,
    getSelectedTaskId: () => options.taskId ?? null,
    getSelectedSessionId: () => options.sessionId ?? null,
    getSelectedWorkflowId: () => options.workflowId ?? null,
    getSelectedWorkflowSessionIds: () => options.workflowSessionIds ?? [],
    loadTasks: async () => { calls.push('tasks'); },
    loadWorkspaces: async () => { calls.push('workspaces'); },
    loadAgentProfiles: async () => { calls.push('profiles'); },
    loadSessions: async () => { calls.push('sessions'); },
    loadWorkflows: async () => { calls.push('workflows'); },
    refreshTask: async (taskId) => { calls.push(`task:${taskId}`); },
    refreshSession: async (sessionId) => { calls.push(`session:${sessionId}`); },
    refreshWorkflow: async (workflowId) => { calls.push(`workflow:${workflowId}`); },
  });
}

test('coalesces bursts of dashboard stream events into one refresh per affected resource', async () => {
  const calls: string[] = [];
  const refreshes = scheduler(calls, { taskId: 'task-1', sessionId: 'session-1' });
  refreshes.handleEvent(taskEvent('task-1'));
  refreshes.handleEvent(taskEvent('task-1'));
  refreshes.handleEvent(sessionEvent());
  await refreshes.flushNow();
  expect(calls.sort()).toEqual(['session:session-1', 'task:task-1', 'tasks', 'workflows'].sort());
});

test('refreshes selected session detail and workflow list for a session event', async () => {
  const calls: string[] = [];
  const refreshes = scheduler(calls, { sessionId: 'session-1' });
  refreshes.handleEvent(sessionEvent());
  await refreshes.flushNow();
  expect(calls.sort()).toEqual(['session:session-1', 'workflows'].sort());
});

test('refreshes selected workflow when the event belongs to one of its sessions', async () => {
  const calls: string[] = [];
  const refreshes = scheduler(calls, { workflowId: 'wf-1', workflowSessionIds: ['session-2'] });
  refreshes.handleEvent(sessionEvent('session.updated', 'session-2'));
  await refreshes.flushNow();
  expect(calls.sort()).toEqual(['sessions', 'workflow:wf-1', 'workflows'].sort());
});

test('ignores high-frequency transcript message updates for projection refreshes', async () => {
  const calls: string[] = [];
  const refreshes = scheduler(calls, { sessionId: 'session-1', workflowId: 'wf-1', workflowSessionIds: ['session-1'] });
  refreshes.handleEvent(sessionEvent('session.message_updated'));
  await refreshes.flushNow();
  expect(calls).toEqual([]);
});
