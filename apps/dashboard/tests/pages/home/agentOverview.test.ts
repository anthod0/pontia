import { expect, test } from 'vitest';
import {
  activeAgentSessions,
  agentOverviewCounts,
  agentStateLabel,
  agentWorkspaceLabel,
  formatAgentUpdatedAt,
  workspacesWithActiveAgents,
} from '../../../src/pages/home/agentOverview';

const session = (state: string, updated_at: string, workspace_id = 'workspace-1') => ({ state, updated_at, workspace_id });

test('shows only active agents with working agents first and recent agents next', () => {
  const agents = activeAgentSessions([
    session('idle', '2026-01-01T00:30:00Z'),
    session('exited', '2026-01-01T00:50:00Z'),
    session('busy', '2026-01-01T00:10:00Z'),
    session('starting', '2026-01-01T00:40:00Z'),
    session('error', '2026-01-01T01:00:00Z'),
  ]);

  expect(agents.map((agent) => agent.state)).toEqual(['busy', 'starting', 'idle']);
});

test('filters active agents to the selected workspace', () => {
  const agents = activeAgentSessions([
    session('busy', '2026-01-01T00:10:00Z', 'workspace-1'),
    session('idle', '2026-01-01T00:20:00Z', 'workspace-2'),
  ], 'workspace-2');

  expect(agents.map((agent) => agent.workspace_id)).toEqual(['workspace-2']);
});

test('keeps only workspaces that currently have active agents', () => {
  const workspaces = [
    { workspace_id: 'workspace-1' },
    { workspace_id: 'workspace-2' },
    { workspace_id: 'workspace-3' },
  ];
  const visible = workspacesWithActiveAgents(workspaces, [
    session('busy', '2026-01-01T00:10:00Z', 'workspace-1'),
    session('exited', '2026-01-01T00:20:00Z', 'workspace-2'),
  ]);

  expect(visible).toEqual([{ workspace_id: 'workspace-1' }]);
});

test('summarizes agent states without treating errors as active agents', () => {
  expect(agentOverviewCounts([
    { state: 'busy' },
    { state: 'busy' },
    { state: 'idle' },
    { state: 'created' },
    { state: 'starting' },
    { state: 'interrupted' },
    { state: 'error' },
    { state: 'exited' },
  ])).toEqual({ working: 2, idle: 1, starting: 2, attention: 2 });
});

test('uses the registered workspace name and falls back to the session workspace path', () => {
  const workspaces = [{ workspace_id: 'workspace-1', name: 'pontia', display_path: '~/repo/pontia' }];
  expect(agentWorkspaceLabel({ workspace_id: 'workspace-1', workspace: '/repo/pontia' }, workspaces)).toBe('pontia');
  expect(agentWorkspaceLabel({ workspace_id: 'missing', workspace: '/repo/other' }, workspaces)).toBe('/repo/other');
});

test('formats status labels and compact updated times', () => {
  const now = new Date('2026-01-02T00:00:00Z').getTime();
  expect(agentStateLabel('busy')).toBe('Working');
  expect(agentStateLabel('custom')).toBe('custom');
  expect(formatAgentUpdatedAt('2026-01-01T23:59:45Z', now)).toBe('just now');
  expect(formatAgentUpdatedAt('2026-01-01T23:45:00Z', now)).toBe('15m ago');
  expect(formatAgentUpdatedAt('2026-01-01T21:00:00Z', now)).toBe('3h ago');
  expect(formatAgentUpdatedAt('2025-12-31T00:00:00Z', now)).toBe('2d ago');
});
