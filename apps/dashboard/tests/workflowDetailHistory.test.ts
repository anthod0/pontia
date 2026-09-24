import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import WorkflowDetailPage from '../src/pages/WorkflowDetailPage.svelte';
import { workflowDetail, workflowDetailError, workflowDetailLoading } from '../src/stores/workflows';
import type { WorkflowDetailView } from '../src/api/types';
const mocks = vi.hoisted(() => ({ getWorkflow: vi.fn(), getWorkflowRevision: vi.fn(), listWorkflowPatches: vi.fn(), retryWorkflow: vi.fn() }));
vi.mock('../src/api/client', () => ({ ...mocks, listWorkflows: vi.fn(), pauseWorkflow: vi.fn(), resumeWorkflow: vi.fn() }));
vi.mock('$lib/navigation', () => ({ navigate: async (path: string, query: Record<string, string | null> = {}, options: { replaceState?: boolean } = {}) => {
  const url = new URL(path, window.location.origin);
  for (const [key, value] of Object.entries(query)) if (value !== null) url.searchParams.set(key, value);
  window.history[options.replaceState ? 'replaceState' : 'pushState']({}, '', url);
  window.dispatchEvent(new PopStateEvent('popstate'));
} }));
const snapshot: WorkflowDetailView = { retry_failure_event_id: null, retry_unavailable_reason: null, recoveries: [], workflow_id: 'wf', title: 'Example', state: 'completed', current_revision: 3, active_patch: null, failure_message: null, cwd: '/workspace', agent_submitted_count: 0, agent_total_count: 1, current_node_id: null, started_at: null, completed_at: null, created_at: '', updated_at: '', elapsed_ms: 0, nodes: [{ node_id: 'n', phase: 'Build', title: 'Current writer', status: 'pending', session_id: 'session', session_state: null, submitted_at: null }] };
beforeEach(() => {
  vi.clearAllMocks(); workflowDetail.set(null); workflowDetailError.set(null); workflowDetailLoading.set(false);
  mocks.getWorkflow.mockResolvedValue(snapshot);
  mocks.listWorkflowPatches.mockResolvedValue([]);
  mocks.getWorkflowRevision.mockImplementation(async (id, revision) => ({ workflow_id: id, revision, current: false, nodes: [] }));
});
function visit(query: string) { window.history.replaceState({}, '', `/workflows/wf${query}`); window.dispatchEvent(new PopStateEvent('popstate')); }

test('Retry keeps the failure identity and shows durable recovery progress', async () => {
  visit('');
  const failed = { ...snapshot, state: 'failed' as const, retry_failure_event_id: 'failure-1', failure_message: 'session.exited before Submission' };
  mocks.getWorkflow.mockResolvedValue(failed);
  let complete!: (value: WorkflowDetailView) => void;
  mocks.retryWorkflow.mockImplementation(() => new Promise((resolve) => { complete = resolve; }));
  render(WorkflowDetailPage, { routeWorkflowId: 'wf' });
  const button = await screen.findByRole('button', { name: 'Retry', exact: true });
  await fireEvent.click(button);
  expect(button).toBeDisabled();
  expect(mocks.retryWorkflow).toHaveBeenCalledExactlyOnceWith('wf', 'failure-1');
  const recovering = { ...failed, state: 'recovering' as const, retry_failure_event_id: null, recoveries: [{ original_failure_message: failed.failure_message, recovery_id: 'r1', failure_event_id: 'failure-1', node_id: 'n', session_id: 'session', message_id: 'message', state: 'requested' as const, failure_message: null, created_at: '2026-09-24' }] };
  mocks.getWorkflow.mockResolvedValue(recovering);
  complete(recovering);
  expect(await screen.findByText('Recovery history')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Retry', exact: true })).not.toBeInTheDocument();
  expect(screen.getByText('session.exited before Submission')).toBeInTheDocument();
});

test('unsupported failures explain why Retry is unavailable', async () => {
  visit('');
  mocks.getWorkflow.mockResolvedValue({ ...snapshot, state: 'failed', retry_unavailable_reason: 'Resolve uncertain input before recovery.' });
  render(WorkflowDetailPage, { routeWorkflowId: 'wf' });
  expect(await screen.findByText('Resolve uncertain input before recovery.')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Retry', exact: true })).not.toBeInTheDocument();
});

test('version buttons and popstate select history independently of the current revision', async () => {
  visit('?revision=2&phase=1');
  const view = render(WorkflowDetailPage, { routeWorkflowId: 'wf' });
  expect(await screen.findByText('Viewing v2')).toBeInTheDocument();
  expect(screen.getByText('Current v3')).toBeInTheDocument();
  workflowDetail.set({ ...snapshot, current_revision: 4 });
  expect(await screen.findByText('Current v4')).toBeInTheDocument();
  expect(screen.getByText('Viewing v2')).toBeInTheDocument();
  expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
  expect(screen.queryByText('Current Replanner')).not.toBeInTheDocument();
  const versions = screen.getByRole('group', { name: 'Workflow versions' });
  expect(versions.compareDocumentPosition(screen.getByText('Viewing v2')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(view.container.querySelectorAll('[data-slot="card"]')).toHaveLength(1);
  await fireEvent.click(screen.getByRole('button', { name: 'v4 Current' }));
  expect(await screen.findByText('Current writer')).toBeInTheDocument();
  expect(new URLSearchParams(window.location.search).has('phase')).toBe(false);
  await fireEvent.click(screen.getByRole('button', { name: 'v1' }));
  expect(await screen.findByText('Viewing v1')).toBeInTheDocument();
  visit('?revision=1&phase=1');
  expect(await screen.findByText('Viewing v1')).toBeInTheDocument();
  view.unmount();
  render(WorkflowDetailPage, { routeWorkflowId: 'wf' });
  expect(await screen.findByText('Viewing v1')).toBeInTheDocument();
});

test('default selection follows the current workflow without fetching history', async () => {
  visit('?phase=1');
  render(WorkflowDetailPage, { routeWorkflowId: 'wf' });
  expect(await screen.findByText('Current writer')).toBeInTheDocument();
  workflowDetail.set({ ...snapshot, current_revision: 4 });
  expect(await screen.findByText('Current v4')).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole('button', { name: 'v4 Current' })).toHaveAttribute('aria-pressed', 'true'));
  expect(mocks.getWorkflowRevision).not.toHaveBeenCalled();
});
