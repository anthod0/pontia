import { get } from 'svelte/store';
import { loadSessions, loadSessionDetail, sessionDetail } from '../stores/sessions';
import { hasTimelineSnapshot, loadSessionTimeline, refreshSessionTimeline, timelineState } from '../stores/timeline';

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
  const refreshes: Promise<unknown>[] = [loadSessions({ showLoading: false })];

  if (selectedSessionId) {
    refreshes.push(loadSessionDetail(selectedSessionId, { showLoading: false }));
    refreshes.push(hasTimelineSnapshot(timeline, selectedSessionId)
      ? refreshSessionTimeline(selectedSessionId, timeline.latestTurnId)
      : loadSessionTimeline(selectedSessionId, { mode: 'rebuild', latestTurnId: timeline.latestTurnId }));
  }

  await Promise.allSettled(refreshes);
}
