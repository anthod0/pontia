import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { Check, Loader, LogOut, Pause, TriangleAlert } from '@lucide/svelte';
import { describe, expect, test, vi } from 'vitest';
import SessionComposerDock from '../src/components/chat/SessionComposerDock.svelte';
import GitStatusInline from '../src/components/chat/GitStatusInline.svelte';
import SessionMetadataMobile from '../src/components/chat/SessionMetadataMobile.svelte';
import {
  sessionMetadataItems,
  sessionMetadataSummary,
  sessionStateBadgeClass,
  sessionStateIcon,
  sessionStateIconClass,
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
    gitStatusErrors: {},
    workspaces,
    metadataItems,
    metadataSummary: sessionMetadataSummary(metadataItems),
  };
}

describe('session metadata component boundaries', () => {
  test('session status badge maps active and interrupted states to requested colors', () => {
    expect(sessionStateBadgeClass('busy')).toContain('bg-amber-500/10');
    expect(sessionStateBadgeClass('starting')).toContain('bg-amber-500/10');
    expect(sessionStateBadgeClass('idle')).toContain('bg-emerald-500/10');
    expect(sessionStateBadgeClass('interrupted')).toContain('bg-emerald-500/10');
    expect(sessionStateBadgeClass('exited')).toContain('bg-muted');
    expect(sessionStateBadgeClass('error')).toContain('bg-destructive/10');
  });

  test('session status badge uses semantic icons per state', () => {
    expect(sessionStateIcon('busy')).toBe(Loader);
    expect(sessionStateIcon('starting')).toBe(Loader);
    expect(sessionStateIcon('idle')).toBe(Check);
    expect(sessionStateIcon('interrupted')).toBe(Pause);
    expect(sessionStateIcon('exited')).toBe(LogOut);
    expect(sessionStateIcon('error')).toBe(TriangleAlert);
    expect(sessionStateIconClass('busy')).toContain('animate-spin');
    expect(sessionStateIconClass('starting')).toContain('animate-spin');
    expect(sessionStateIconClass('idle')).not.toContain('animate-spin');
  });

  test('composer dock renders compact metadata and opens advanced controls from the component menu', async () => {
    render(SessionComposerDock, {
      props: {
        ...metadataProps(),
        inboxActionableCount: 2,
        input: '',
        onOpenInbox: vi.fn(),
        onExit: vi.fn(),
        onOpenConsole: vi.fn(),
        onNewChat: vi.fn(),
        onRename: vi.fn(),
        onRestart: vi.fn(),
        onSend: vi.fn(),
        onFocus: vi.fn(),
      },
    });

    expect(screen.getByRole('group', { name: 'Session status and controls' })).toBeInTheDocument();
    const composerDock = document.querySelector('[data-chat-composer-dock="fixed"]');
    expect(composerDock).toBeInTheDocument();
    expect(composerDock).toHaveClass('bg-surface');
    expect(composerDock).toHaveClass('pt-1');
    expect(composerDock).not.toHaveClass('md:pt-2');
    expect(composerDock).not.toHaveClass('bg-muted/20');
    expect(composerDock).not.toHaveClass('backdrop-blur');
    expect(composerDock).not.toHaveClass('[mask-image:linear-gradient(to_bottom,transparent,black_18px)]');
    const outwardFade = document.querySelector('[data-chat-composer-fade="outward"]');
    expect(outwardFade).toBeInTheDocument();
    expect(outwardFade).toHaveClass('h-12');
    expect(outwardFade).toHaveClass('bg-gradient-to-t');
    expect(outwardFade).toHaveClass('from-surface');
    expect(outwardFade).toHaveClass('via-surface/70');
    expect(outwardFade).toHaveClass('to-transparent');
    expect(screen.getByRole('button', { name: /Session details: pontia · pi · main · dirty · 33% · 42k \/ 128k · coder@1 · main/ })).toBeInTheDocument();
    expect(screen.queryByLabelText(/session state:/i)).not.toBeInTheDocument();
    expect(screen.queryByText('Session state:')).not.toBeInTheDocument();

    const primaryActions = screen.getByRole('group', { name: /primary session actions/i });
    expect(primaryActions).toHaveClass('flex');
    expect(within(primaryActions).getByRole('button', { name: /new chat/i })).toBeInTheDocument();
    expect(within(primaryActions).getByRole('button', { name: /open inbox, 2 messages/i })).toBeInTheDocument();
    expect(within(primaryActions).getByRole('button', { name: /exit session/i })).toBeInTheDocument();
    expect(within(primaryActions).getByRole('button', { name: /advanced session controls/i })).toBeInTheDocument();
    expect(Array.from(primaryActions.children).map((child) => child.getAttribute('data-slot'))).toEqual(['button', 'button', 'button', 'dropdown-menu-trigger']);
    for (const button of within(primaryActions).getAllByRole('button')) {
      expect(button).toHaveClass('hover:bg-muted');
      expect(button).not.toHaveClass('border');
    }

    await fireEvent.click(screen.getByRole('button', { name: /advanced session controls/i }));

    expect((await screen.findAllByRole('menuitem')).map((item) => item.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
      'New Chat',
      'Inbox 2',
      'Rename session',
      'Restart session',
      'Session Console',
      'Exit session',
    ]);
    expect(document.querySelectorAll('[data-slot="dropdown-menu-separator"]')).toHaveLength(2);
  });

  test('mobile metadata details render as an accessible popover dialog', async () => {
    render(SessionMetadataMobile, { props: metadataProps() });

    await fireEvent.click(screen.getByRole('button', { name: /Session details: pontia · pi · main · dirty · 33% · 42k \/ 128k · coder@1 · main/ }));

    const dialog = await screen.findByRole('dialog', { name: 'Session details' });
    expect(within(dialog).getByLabelText('Workspace: /home/cheny/projects/pontia')).toHaveTextContent('pontia');
    expect(within(dialog).getByLabelText('Git: Git status: main, dirty')).toHaveTextContent('main');
    expect(within(dialog).getByLabelText(/Usage: Context usage: 33%/)).toHaveTextContent('33%');
    expect(within(dialog).getByLabelText('Client: pi')).toHaveTextContent('pi');
    expect(within(dialog).getByLabelText('Profile: coder@1')).toHaveTextContent('coder@1');
    expect(within(dialog).getByLabelText('Handle: main')).toHaveTextContent('main');
  });

  test('git inline status leaves the branch label untoned while coloring only counters', () => {
    render(GitStatusInline, { props: { gitStatus: gitStatus({ ahead: 2, unstaged_count: 1, clean: false }) } });

    const branch = screen.getByText('main');
    expect(branch).not.toHaveClass('text-amber-600');
    expect(branch).not.toHaveClass('text-emerald-600');
    expect(screen.getByText('↑2')).toHaveClass('text-blue-600');
    expect(screen.getByText('~1')).toHaveClass('text-amber-600');
  });

  test('session metadata trigger shows desktop-only icons inline with summary fields', () => {
    render(SessionMetadataMobile, { props: metadataProps() });

    const trigger = screen.getByRole('button', { name: /Session details: pontia · pi · main · dirty · 33% · 42k \/ 128k · coder@1 · main/ });
    for (const iconClass of ['lucide-folder', 'lucide-git-branch', 'lucide-gauge', 'lucide-terminal', 'lucide-bot', 'lucide-at-sign']) {
      const icon = trigger.querySelector(`.${iconClass}`);
      expect(icon).toBeInTheDocument();
      expect(icon).toHaveClass('hidden');
      expect(icon).toHaveClass('sm:inline');
    }
  });
});
