import { get } from 'svelte/store';
import { loadAgentProfiles } from '../stores/agentProfiles';
import { loadSessions, loadSessionDetail, selectedSessionId } from '../stores/sessions';
import { loadTasks, refreshTask, selectedTaskId } from '../stores/tasks';
import { hasTimelineSnapshot, loadSessionTimeline, refreshSessionTimeline, timelineState } from '../stores/timeline';
import { loadWorkspaces } from '../stores/workspaces';
import { loadWorkflows, refreshWorkflow, selectedWorkflowId } from '../stores/workflows';

export type DashboardSnapshotRefreshReason = 'sse_open' | 'sse_fallback';

export type DashboardSnapshotRefreshOptions = {
  reason: DashboardSnapshotRefreshReason;
};

let refreshInFlight: Promise<void> | null = null;
let refreshPending = false;

export function refreshDashboardSnapshot(_options: DashboardSnapshotRefreshOptions): Promise<void> {
  refreshPending = true;
  if (refreshInFlight) return refreshInFlight;

  refreshInFlight = (async () => {
    do {
      refreshPending = false;
      await refreshDashboardSnapshotNow();
    } while (refreshPending);
  })().finally(() => {
    refreshInFlight = null;
  });
  return refreshInFlight;
}

async function refreshSelectedSession(sessionId: string): Promise<void> {
  const detail = await loadSessionDetail(sessionId, { showLoading: false });
  if (get(selectedSessionId) !== sessionId || !detail?.session.capabilities.timeline) return;
  const timeline = get(timelineState);
  const topology = detail.session.capabilities.topology === true;
  const latestTurnId = detail.turns.reduce<string | null>(
    (latest, turn) => latest === null || turn.turn_id > latest ? turn.turn_id : latest,
    null,
  );
  if (hasTimelineSnapshot(timeline, sessionId) && timeline.mode === (topology ? 'tree' : 'linear')) {
    await refreshSessionTimeline(sessionId, timeline.latestTurnId ?? latestTurnId);
  } else {
    await loadSessionTimeline(sessionId, { mode: 'rebuild', latestTurnId, topology });
  }
}

async function refreshDashboardSnapshotNow(): Promise<void> {
  const sessionId = get(selectedSessionId);
  const taskId = get(selectedTaskId);
  const workflowId = get(selectedWorkflowId);
  const refreshes: Promise<unknown>[] = [
    loadSessions({ showLoading: false }),
    loadWorkspaces(),
    loadAgentProfiles(),
    loadTasks(),
    loadWorkflows({ showLoading: false }),
  ];

  if (taskId) refreshes.push(refreshTask(taskId));
  if (workflowId) refreshes.push(refreshWorkflow(workflowId, { showLoading: false }));
  if (sessionId) refreshes.push(refreshSelectedSession(sessionId));

  await Promise.allSettled(refreshes);
}
