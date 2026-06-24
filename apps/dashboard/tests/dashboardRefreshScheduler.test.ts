import { expect, test } from 'vitest';
import { createDashboardRefreshScheduler } from '../src/services/dashboardRefreshScheduler.ts';
import type { DashboardStreamEvent } from '../src/api/types.ts';

function taskEvent(taskId: string): DashboardStreamEvent {
  return {
    kind: 'task_event',
    id: `event-${taskId}`,
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: `event-${taskId}`,
      task_id: taskId,
      event_type: 'dag.work_item_completed',
      payload: {},
      created_at: '2026-05-14T00:00:00Z',
    },
  };
}

function sessionEvent(type = 'session.updated'): DashboardStreamEvent {
  return {
    kind: 'session_event',
    id: `event-session-${type}`,
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: `event-session-${type}`,
      session_id: 'session-1',
      turn_id: null,
      source: 'runtime',
      type,
      time: '2026-05-14T00:00:00Z',
      payload: {},
    },
  };
}

test('coalesces bursts of dashboard stream events into one refresh per affected resource', async () => {
  const calls: string[] = [];
  const scheduler = createDashboardRefreshScheduler({
    delayMs: 0,
    getSelectedTaskId: () => 'task-1',
    getSelectedSessionId: () => 'session-1',
    loadTasks: async () => { calls.push('tasks'); },
    loadWorkspaces: async () => { calls.push('workspaces'); },
    loadAgentProfiles: async () => { calls.push('profiles'); },
    loadSessions: async () => { calls.push('sessions'); },
    refreshTask: async (taskId) => { calls.push(`task:${taskId}`); },
    refreshSession: async (sessionId) => { calls.push(`session:${sessionId}`); },
  });

  scheduler.handleEvent(taskEvent('task-1'));
  scheduler.handleEvent(taskEvent('task-1'));
  scheduler.handleEvent(sessionEvent());
  await scheduler.flushNow();

  expect(calls.sort()).toEqual(['session:session-1', 'task:task-1', 'tasks'].sort());
});

test('refreshes selected session detail without reloading the whole session list when a session stream event arrives', async () => {
  const calls: string[] = [];
  const scheduler = createDashboardRefreshScheduler({
    delayMs: 0,
    getSelectedTaskId: () => null,
    getSelectedSessionId: () => 'session-1',
    loadTasks: async () => { calls.push('tasks'); },
    loadWorkspaces: async () => { calls.push('workspaces'); },
    loadAgentProfiles: async () => { calls.push('profiles'); },
    loadSessions: async () => { calls.push('sessions'); },
    refreshTask: async (taskId) => { calls.push(`task:${taskId}`); },
    refreshSession: async (sessionId) => { calls.push(`session:${sessionId}`); },
  });

  scheduler.handleEvent(sessionEvent());
  await scheduler.flushNow();

  expect(calls).toEqual(['session:session-1']);
});

test('ignores high-frequency transcript message updates for projection refreshes', async () => {
  const calls: string[] = [];
  const scheduler = createDashboardRefreshScheduler({
    delayMs: 0,
    getSelectedTaskId: () => null,
    getSelectedSessionId: () => 'session-1',
    loadTasks: async () => { calls.push('tasks'); },
    loadWorkspaces: async () => { calls.push('workspaces'); },
    loadAgentProfiles: async () => { calls.push('profiles'); },
    loadSessions: async () => { calls.push('sessions'); },
    refreshTask: async (taskId) => { calls.push(`task:${taskId}`); },
    refreshSession: async (sessionId) => { calls.push(`session:${sessionId}`); },
  });

  scheduler.handleEvent(sessionEvent('session.message_updated'));
  scheduler.handleEvent(sessionEvent('session.message_updated'));
  await scheduler.flushNow();

  expect(calls).toEqual([]);
});
