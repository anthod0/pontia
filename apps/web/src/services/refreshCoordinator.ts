import { get } from 'svelte/store';
import { loadArtifacts } from '../stores/artifacts';
import { selectedSessionId } from '../stores/selection';
import { refreshSession } from '../stores/sessionDetail';
import { loadSessions } from '../stores/sessions';
import { loadInboxMessages } from '../stores/inbox';
import { loadTurns } from '../stores/turns';
import { loadTasks, refreshTask, selectedTaskId } from '../stores/tasks';

type RefreshTask = () => Promise<void>;

function coalesce(delayMs: number, task: RefreshTask): () => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let running = false;
  let rerun = false;

  async function run() {
    timer = null;
    if (running) {
      rerun = true;
      return;
    }
    running = true;
    try {
      await task();
    } finally {
      running = false;
      if (rerun) {
        rerun = false;
        timer = setTimeout(run, delayMs);
      }
    }
  }

  return () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(run, delayMs);
  };
}

export const refreshSelectedSession = coalesce(100, async () => {
  const id = get(selectedSessionId);
  if (id) await refreshSession(id);
});

export const refreshTurns = coalesce(150, async () => {
  const id = get(selectedSessionId);
  if (id) await loadTurns(id);
});

export const refreshInboxMessages = coalesce(150, async () => {
  const id = get(selectedSessionId);
  if (id) await loadInboxMessages(id);
});

export const refreshSessionList = coalesce(250, loadSessions);

export const refreshArtifacts = coalesce(250, async () => {
  const id = get(selectedSessionId);
  if (id) await loadArtifacts(id);
});

export const refreshTaskList = coalesce(250, loadTasks);

export const refreshSelectedTask = coalesce(150, async () => {
  const id = get(selectedTaskId);
  if (id) await refreshTask(id);
});
