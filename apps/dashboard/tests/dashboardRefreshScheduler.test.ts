import assert from 'node:assert/strict';
import test from 'node:test';
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

function sessionEvent(): DashboardStreamEvent {
  return {
    kind: 'session_event',
    id: 'event-session',
    occurred_at: '2026-05-14T00:00:00Z',
    event: {
      event_id: 'event-session',
      session_id: 'session-1',
      turn_id: null,
      source: 'runtime',
      type: 'session.updated',
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
    loadTasks: async () => { calls.push('tasks'); },
    loadWorkspaces: async () => { calls.push('workspaces'); },
    loadAgentProfiles: async () => { calls.push('profiles'); },
    refreshTask: async (taskId) => { calls.push(`task:${taskId}`); },
  });

  scheduler.handleEvent(taskEvent('task-1'));
  scheduler.handleEvent(taskEvent('task-1'));
  scheduler.handleEvent(sessionEvent());
  await scheduler.flushNow();

  assert.deepEqual(calls.sort(), ['profiles', 'task:task-1', 'tasks', 'workspaces'].sort());
});
