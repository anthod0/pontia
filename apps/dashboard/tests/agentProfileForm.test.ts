import { expect, test } from 'vitest';
import { buildAgentProfileInput, createAgentProfileDraft, createAgentProfileDraftFromProfile } from '../src/pages/agentProfiles/form.ts';
import type { AgentProfileView } from '../src/api/types.ts';

const profile: AgentProfileView = {
  profile_id: 'reviewer',
  version: 'v1',
  name: 'Reviewer',
  description: 'Reviews work',
  supported_client_types: ['pi'],
  agent_kind: 'executor',
  system_prompt_template: 'system',
  turn_prompt_template: 'turn',
  default_session_role: 'reviewer',
  default_session_description: 'review session',
  handle_prefix: 'review',
  expected_output_schema: '{"type":"object"}',
  artifact_contract: { kind: 'patch' },
  default_execution_policy: { timeout: 60 },
  default_review_policy: { required: true },
  metadata: { owner: 'test' },
  active: true,
  archived_at: null,
  archived_reason: null,
  created_at: '2026-05-15T00:00:00Z',
  updated_at: '2026-05-15T00:00:00Z',
};

test('creates a version draft by copying the current profile and clearing only the version', () => {
  const draft = createAgentProfileDraftFromProfile(profile, { clearVersion: true });

  expect(draft.profile_id).toBe('reviewer');
  expect(draft.version).toBe('');
  expect(draft.name).toBe('Reviewer');
  expect(draft.supported_client_types_text).toBe('pi');
  expect(draft.agent_kind).toBe('executor');
  expect(draft.artifact_contract_text).toMatch(/"kind": "patch"/);
});

test('defaults new agent profiles to the active pi client only', () => {
  const draft = createAgentProfileDraft();

  expect(draft.supported_client_types_text).toBe('pi');
});

test('builds an upsert input with parsed JSON fields and split client types', () => {
  const draft = createAgentProfileDraft();
  draft.profile_id = 'coder';
  draft.version = 'v1';
  draft.name = 'Coder';
  draft.supported_client_types_text = 'pi, pi';
  draft.description = '';
  draft.artifact_contract_text = '{"outputs":["patch"]}';

  const result = buildAgentProfileInput(draft);

  expect(result.ok).toBe(true);
  if (!result.ok) return;
  expect(result.input.supported_client_types).toEqual(['pi']);
  expect(result.input.agent_kind).toBe('executor');
  expect(result.input.description).toBe(null);
  expect(result.input.artifact_contract).toEqual({ outputs: ['patch'] });
});

test('returns field errors without building input when required or JSON fields are invalid', () => {
  const draft = createAgentProfileDraft();
  draft.version = 'v1';
  draft.name = 'Broken';
  draft.metadata_text = '{not-json';

  const result = buildAgentProfileInput(draft);

  expect(result.ok).toBe(false);
  if (result.ok) return;
  expect(result.errors.profile_id).toBe('Profile ID is required.');
  expect(result.errors.metadata_text ?? '').toMatch(/Invalid JSON/);
});
