import type { DashboardStreamEvent } from '../api/types';

type RefreshOptions = {
  delayMs?: number;
  getSelectedTaskId: () => string | null;
  getSelectedSessionId: () => string | null;
  loadTasks: () => Promise<unknown>;
  loadWorkspaces: () => Promise<unknown>;
  loadAgentProfiles: () => Promise<unknown>;
  loadSessions: () => Promise<unknown>;
  refreshTask: (taskId: string) => Promise<unknown>;
  refreshSession: (sessionId: string) => Promise<unknown>;
};

type PendingRefresh = {
  tasks: boolean;
  workspaces: boolean;
  agentProfiles: boolean;
  sessions: boolean;
  taskIds: Set<string>;
  sessionIds: Set<string>;
};

function emptyPending(): PendingRefresh {
  return {
    tasks: false,
    workspaces: false,
    agentProfiles: false,
    sessions: false,
    taskIds: new Set(),
    sessionIds: new Set(),
  };
}

function hasPending(pending: PendingRefresh): boolean {
  return pending.tasks || pending.workspaces || pending.agentProfiles || pending.sessions || pending.taskIds.size > 0 || pending.sessionIds.size > 0;
}

export function createDashboardRefreshScheduler(options: RefreshOptions) {
  const delayMs = options.delayMs ?? 250;
  let pending = emptyPending();
  let timer: ReturnType<typeof setTimeout> | null = null;
  let flushing = false;

  function clearTimer(): void {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function schedule(): void {
    if (timer || flushing) return;
    timer = setTimeout(() => {
      timer = null;
      void flushNow();
    }, delayMs);
  }

  async function flushNow(): Promise<void> {
    clearTimer();
    if (flushing) return;
    if (!hasPending(pending)) return;

    flushing = true;
    const batch = pending;
    pending = emptyPending();

    try {
      const refreshes: Promise<unknown>[] = [];
      if (batch.tasks) refreshes.push(options.loadTasks());
      if (batch.workspaces) refreshes.push(options.loadWorkspaces());
      if (batch.agentProfiles) refreshes.push(options.loadAgentProfiles());
      if (batch.sessions) refreshes.push(options.loadSessions());
      for (const taskId of batch.taskIds) refreshes.push(options.refreshTask(taskId));
      for (const sessionId of batch.sessionIds) refreshes.push(options.refreshSession(sessionId));
      await Promise.all(refreshes);
    } finally {
      flushing = false;
      if (hasPending(pending)) schedule();
    }
  }

  function handleEvent(streamEvent: DashboardStreamEvent): void {
    if (streamEvent.kind === 'task_event') {
      pending.tasks = true;
      const selected = options.getSelectedTaskId();
      if (selected && streamEvent.event.task_id === selected) pending.taskIds.add(selected);
    } else if (streamEvent.kind === 'session_event') {
      pending.sessions = true;
      const selected = options.getSelectedSessionId();
      if (selected && streamEvent.event.session_id === selected) pending.sessionIds.add(selected);
    }

    schedule();
  }

  function reset(): void {
    clearTimer();
    pending = emptyPending();
    flushing = false;
  }

  return { handleEvent, flushNow, reset };
}
