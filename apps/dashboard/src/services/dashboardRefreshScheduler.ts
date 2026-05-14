import type { DashboardStreamEvent } from '../api/types';

type RefreshOptions = {
  delayMs?: number;
  getSelectedTaskId: () => string | null;
  loadTasks: () => Promise<void>;
  loadWorkspaces: () => Promise<void>;
  loadAgentProfiles: () => Promise<void>;
  refreshTask: (taskId: string) => Promise<void>;
};

type PendingRefresh = {
  tasks: boolean;
  workspaces: boolean;
  agentProfiles: boolean;
  taskIds: Set<string>;
};

function emptyPending(): PendingRefresh {
  return {
    tasks: false,
    workspaces: false,
    agentProfiles: false,
    taskIds: new Set(),
  };
}

function hasPending(pending: PendingRefresh): boolean {
  return pending.tasks || pending.workspaces || pending.agentProfiles || pending.taskIds.size > 0;
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
      const refreshes: Promise<void>[] = [];
      if (batch.tasks) refreshes.push(options.loadTasks());
      if (batch.workspaces) refreshes.push(options.loadWorkspaces());
      if (batch.agentProfiles) refreshes.push(options.loadAgentProfiles());
      for (const taskId of batch.taskIds) refreshes.push(options.refreshTask(taskId));
      await Promise.all(refreshes);
    } finally {
      flushing = false;
      if (hasPending(pending)) schedule();
    }
  }

  function handleEvent(streamEvent: DashboardStreamEvent): void {
    pending.tasks = true;

    if (streamEvent.kind === 'task_event') {
      const selected = options.getSelectedTaskId();
      if (selected && streamEvent.event.task_id === selected) pending.taskIds.add(selected);
    } else {
      pending.workspaces = true;
      pending.agentProfiles = true;
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
