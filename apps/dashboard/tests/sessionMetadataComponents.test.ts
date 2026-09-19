import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, test, vi } from 'vitest';
import SessionComposerDock from '../src/components/chat/SessionComposerDock.svelte';
import SessionMetadata from '../src/components/chat/SessionMetadata.svelte';
import {
  sessionMetadataItems,
  sessionMetadataSummary,
} from '../src/components/chat/sessionMetadata';
import type { SessionView, WorkspaceGitStatusView, WorkspaceView } from '../src/api/types';

function session(overrides: Partial<SessionView> = {}): SessionView {
  return {
    session_id: 'session-1',
    client_type: 'pi',
    title: null,
    handle: 'main',
    role: 'coder',
    description: null,
    execution_profile_id: 'coder',
    execution_profile_version: '1',
    state: 'idle',
    current_turn_id: null,
    workspace_id: 'workspace-1',
    workspace: '/home/cheny/projects/pontia',
    pinned_at: null,
    archived_at: null,
    capabilities: { accept_task: true, context_usage: 'exact' },
    model: null,
    context_usage: {
      used_tokens: 42000,
      max_tokens: 128000,
      remaining_tokens: 86000,
      usage_ratio: 0.328125,
      input_tokens: null,
      output_tokens: null,
      cache_tokens: null,
      confidence: 'exact',
      observed_at: '2026-06-11T00:00:00Z',
    },
    lineage: null,
    created_at: '2026-06-11T00:00:00Z',
    updated_at: '2026-06-11T00:00:00Z',
    metadata: {},
    ...overrides,
  };
}

function workspace(overrides: Partial<WorkspaceView> = {}): WorkspaceView {
  return {
    workspace_id: 'workspace-1',
    canonical_path: '/home/cheny/projects/pontia',
    display_path: '~/projects/pontia',
    name: 'pontia',
    state: 'active',
    metadata: {},
    created_at: '2026-06-11T00:00:00Z',
    updated_at: '2026-06-11T00:00:00Z',
    last_used_at: '2026-06-11T00:00:00Z',
    ...overrides,
  };
}

function gitStatus(overrides: Partial<WorkspaceGitStatusView> = {}): WorkspaceGitStatusView {
  return {
    workspace_id: 'workspace-1',
    repo_root: '/home/cheny/projects/pontia',
    branch: 'main',
    upstream: 'origin/main',
    ahead: 0,
    behind: 0,
    staged_count: 0,
    unstaged_count: 0,
    untracked_count: 0,
    conflicted_count: 0,
    clean: true,
    state: 'observed',
    failure: null,
    observed_at: '2026-06-11T00:00:00Z',
    updated_at: '2026-06-11T00:00:00Z',
    ...overrides,
  };
}

function metadataProps() {
  const currentSession = session();
  const workspaces = [workspace()];
  const currentGitStatus = gitStatus({ unstaged_count: 1, clean: false });
  const metadataItems = sessionMetadataItems(currentSession, workspaces, currentGitStatus, {});

  return {
    session: currentSession,
    gitStatus: currentGitStatus,
    workspaces,
    metadataItems,
    metadataSummary: sessionMetadataSummary(metadataItems),
  };
}

describe('session metadata component boundaries', () => {


  test('composer dock shows metadata without session action buttons', () => {
    render(SessionComposerDock, {
      props: {
        ...metadataProps(),
        queuedMessages: [],
        inboxBusyMessageId: null,
        input: '',
        onCancelInboxMessage: vi.fn(),
        onRetryInboxMessage: vi.fn(),
        onDismissInboxMessage: vi.fn(),
        onSend: vi.fn(),
        onInterrupt: vi.fn(),
        onFocus: vi.fn(),
      },
    });

    expect(screen.getByRole('button', { name: /Session details: pontia · pi · main · dirty · 33% · 42k \/ 128k · coder@1 · main/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /exit session/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /new chat/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /advanced session controls/i })).not.toBeInTheDocument();
  });

  test('session metadata details render as an accessible popover dialog', async () => {
    render(SessionMetadata, { props: metadataProps() });

    await fireEvent.click(screen.getByRole('button', { name: /Session details: pontia · pi · main · dirty · 33% · 42k \/ 128k · coder@1 · main/ }));

    const dialog = await screen.findByRole('dialog', { name: 'Session details' });
    expect(within(dialog).getByLabelText('Workspace: /home/cheny/projects/pontia')).toHaveTextContent('pontia');
    expect(within(dialog).getByLabelText('Git: Git status: main, dirty')).toHaveTextContent('main');
    expect(within(dialog).getByLabelText(/Usage: Context usage: 33%/)).toHaveTextContent('33%');
    expect(within(dialog).getByLabelText('Client: pi')).toHaveTextContent('pi');
    expect(within(dialog).getByLabelText('Profile: coder@1')).toHaveTextContent('coder@1');
    expect(within(dialog).getByLabelText('Handle: main')).toHaveTextContent('main');
  });


});
