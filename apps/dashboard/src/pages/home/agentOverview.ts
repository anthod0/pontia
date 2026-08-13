import type { SessionView, WorkspaceView } from '../../api/types';

const terminalStates = new Set(['exited', 'error']);

export interface AgentOverviewCounts {
  working: number;
  idle: number;
  starting: number;
  attention: number;
}

export function activeAgentSessions<T extends Pick<SessionView, 'state' | 'updated_at' | 'workspace_id'>>(
  sessions: T[],
  workspaceId: string | null = null,
): T[] {
  return sessions
    .filter((session) => !terminalStates.has(session.state) && (!workspaceId || session.workspace_id === workspaceId))
    .sort((a, b) => {
      if (a.state === 'busy' && b.state !== 'busy') return -1;
      if (a.state !== 'busy' && b.state === 'busy') return 1;
      return b.updated_at.localeCompare(a.updated_at);
    });
}

export function workspacesWithActiveAgents<T extends Pick<WorkspaceView, 'workspace_id'>>(
  workspaces: T[],
  sessions: Array<Pick<SessionView, 'state' | 'updated_at' | 'workspace_id'>>,
): T[] {
  const activeWorkspaceIds = new Set(activeAgentSessions(sessions).map((session) => session.workspace_id));
  return workspaces.filter((workspace) => activeWorkspaceIds.has(workspace.workspace_id));
}

export function agentOverviewCounts(sessions: Array<Pick<SessionView, 'state'>>): AgentOverviewCounts {
  return {
    working: sessions.filter((session) => session.state === 'busy').length,
    idle: sessions.filter((session) => session.state === 'idle').length,
    starting: sessions.filter((session) => session.state === 'created' || session.state === 'starting').length,
    attention: sessions.filter((session) => session.state === 'interrupted' || session.state === 'error').length,
  };
}

export function agentStateLabel(state: string): string {
  switch (state) {
    case 'busy': return 'Working';
    case 'idle': return 'Idle';
    case 'created': return 'Created';
    case 'starting': return 'Starting';
    case 'interrupted': return 'Interrupted';
    case 'error': return 'Error';
    case 'exited': return 'Exited';
    default: return state;
  }
}

export function agentWorkspaceLabel(
  session: Pick<SessionView, 'workspace_id' | 'workspace'>,
  workspaces: Array<Pick<WorkspaceView, 'workspace_id' | 'name' | 'display_path'>>,
): string {
  const workspace = workspaces.find((candidate) => candidate.workspace_id === session.workspace_id);
  return workspace?.name?.trim() || workspace?.display_path || session.workspace || 'No workspace';
}

export function formatAgentUpdatedAt(value: string, now = Date.now()): string {
  const timestamp = new Date(value).getTime();
  if (Number.isNaN(timestamp)) return value;

  const elapsedSeconds = Math.max(0, Math.floor((now - timestamp) / 1000));
  if (elapsedSeconds < 60) return 'just now';
  const elapsedMinutes = Math.floor(elapsedSeconds / 60);
  if (elapsedMinutes < 60) return `${elapsedMinutes}m ago`;
  const elapsedHours = Math.floor(elapsedMinutes / 60);
  if (elapsedHours < 24) return `${elapsedHours}h ago`;
  const elapsedDays = Math.floor(elapsedHours / 24);
  return `${elapsedDays}d ago`;
}
