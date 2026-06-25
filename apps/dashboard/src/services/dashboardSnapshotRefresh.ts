import { get } from 'svelte/store';
import { loadSessions, loadSessionDetail, sessionDetail } from '../stores/sessions';
import { handleTimelineMessageUpdated, loadSessionTimeline, timelineState } from '../stores/timeline';
import { refreshWorkspaceGitStatus } from '../stores/workspaces';

export type DashboardSnapshotRefreshReason = 'online' | 'sse_open' | 'sse_fallback';

export type DashboardSnapshotRefreshOptions = {
  reason: DashboardSnapshotRefreshReason;
};

let refreshInFlight: Promise<void> | null = null;

export function refreshDashboardSnapshot(options: DashboardSnapshotRefreshOptions): Promise<void> {
  if (refreshInFlight) return refreshInFlight;

  refreshInFlight = refreshDashboardSnapshotNow(options).finally(() => {
    refreshInFlight = null;
  });
  return refreshInFlight;
}

async function refreshDashboardSnapshotNow(_options: DashboardSnapshotRefreshOptions): Promise<void> {
  const detail = get(sessionDetail);
  const timeline = get(timelineState);
  const selectedSessionId = detail?.session.session_id ?? timeline.sessionId ?? null;
  const workspaceId = detail?.session.workspace_id ?? null;
  const refreshes: Promise<unknown>[] = [loadSessions({ showLoading: false })];

  if (selectedSessionId) {
    refreshes.push(loadSessionDetail(selectedSessionId, { showLoading: false }));
    refreshes.push(timeline.sessionId === selectedSessionId && timeline.items.length
      ? handleTimelineMessageUpdated(selectedSessionId)
      : loadSessionTimeline(selectedSessionId, { mode: 'rebuild' }));
  }

  if (workspaceId) refreshes.push(refreshWorkspaceGitStatus(workspaceId));

  await Promise.allSettled(refreshes);
}
