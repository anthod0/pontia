import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import AgentProfilesPage from '../src/pages/AgentProfilesPage.svelte';
import type { AgentProfileView } from '../src/api/types';

const mocks = vi.hoisted(() => {
  function writableStore<T>(initial: T) {
    let value = initial;
    const subscribers = new Set<(value: T) => void>();
    return {
      subscribe(run: (value: T) => void) {
        subscribers.add(run);
        run(value);
        return () => subscribers.delete(run);
      },
      set(next: T) {
        value = next;
        for (const run of subscribers) run(value);
      },
    };
  }

  const agentProfiles = writableStore<AgentProfileView[]>([]);
  const agentProfilesLoading = writableStore(false);
  const agentProfilesError = writableStore<string | null>(null);

  return {
    agentProfiles,
    agentProfilesLoading,
    agentProfilesError,
    loadAgentProfiles: vi.fn(async () => undefined),
    listAgentProfileVersions: vi.fn(async () => [] as AgentProfileView[]),
    createAgentProfile: vi.fn(),
    createAgentProfileVersion: vi.fn(),
    deleteAgentProfile: vi.fn(),
    deleteAgentProfileVersion: vi.fn(),
    updateAgentProfileVersion: vi.fn(),
  };
});

vi.mock('../src/stores/agentProfiles', () => ({
  agentProfiles: mocks.agentProfiles,
  agentProfilesLoading: mocks.agentProfilesLoading,
  agentProfilesError: mocks.agentProfilesError,
  loadAgentProfiles: mocks.loadAgentProfiles,
}));

vi.mock('../src/api/client', () => ({
  listAgentProfileVersions: mocks.listAgentProfileVersions,
  createAgentProfile: mocks.createAgentProfile,
  createAgentProfileVersion: mocks.createAgentProfileVersion,
  deleteAgentProfile: mocks.deleteAgentProfile,
  deleteAgentProfileVersion: mocks.deleteAgentProfileVersion,
  updateAgentProfileVersion: mocks.updateAgentProfileVersion,
}));

const profile = (overrides: Partial<AgentProfileView> = {}): AgentProfileView => ({
  profile_id: 'executor',
  version: '1.0.0',
  name: 'Executor',
  description: 'Runs coding tasks',
  agent_kind: 'executor',
  supported_client_types: ['pi'],
  default_session_role: 'executor',
  handle_prefix: null,
  default_session_description: null,
  system_prompt_template: null,
  turn_prompt_template: null,
  expected_output_schema: null,
  artifact_contract: {},
  default_execution_policy: {},
  default_review_policy: {},
  metadata: {},
  active: true,
  created_at: '2026-05-14T00:00:00Z',
  updated_at: '2026-05-14T00:00:00Z',
  ...overrides,
});

beforeEach(() => {
  mocks.agentProfiles.set([]);
  mocks.agentProfilesLoading.set(false);
  mocks.agentProfilesError.set(null);
  vi.clearAllMocks();
});

afterEach(() => {
  document.body.style.pointerEvents = '';
});

test('uses alert dialog instead of window confirm for destructive profile version actions', async () => {
  const user = userEvent.setup();
  const activeProfile = profile();
  const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);
  mocks.agentProfiles.set([activeProfile]);
  mocks.listAgentProfileVersions.mockResolvedValue([activeProfile]);

  render(AgentProfilesPage);

  await screen.findByRole('button', { name: /Delete profile/i });
  await user.click(screen.getByRole('button', { name: /Delete version/i }));

  expect(confirmSpy).not.toHaveBeenCalled();
  expect(screen.getByRole('alertdialog', { name: 'Archive profile version?' })).toBeInTheDocument();
  expect(screen.getByText('Archive version executor@1.0.0?')).toBeInTheDocument();

  confirmSpy.mockRestore();
});

test('uses alert dialog instead of window confirm for destructive profile actions', async () => {
  const user = userEvent.setup();
  const activeProfile = profile();
  const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);
  mocks.agentProfiles.set([activeProfile]);
  mocks.listAgentProfileVersions.mockResolvedValue([activeProfile]);

  render(AgentProfilesPage);

  await screen.findByRole('button', { name: /Delete profile/i });
  await user.click(screen.getByRole('button', { name: /Delete profile/i }));

  expect(confirmSpy).not.toHaveBeenCalled();
  expect(screen.getByRole('alertdialog', { name: 'Archive profile?' })).toBeInTheDocument();
  expect(screen.getByText('Archive profile executor and all active versions?')).toBeInTheDocument();

  confirmSpy.mockRestore();
});

test('aborts initial settings agent profile requests when the page unmounts', async () => {
  const { unmount } = render(AgentProfilesPage);

  await vi.waitFor(() => expect(mocks.loadAgentProfiles).toHaveBeenCalled());
  const [, options] = mocks.loadAgentProfiles.mock.calls[0] as [boolean, { signal?: AbortSignal } | undefined];

  expect(options?.signal).toBeInstanceOf(AbortSignal);
  expect(options?.signal?.aborted).toBe(false);

  unmount();

  expect(options?.signal?.aborted).toBe(true);
});
