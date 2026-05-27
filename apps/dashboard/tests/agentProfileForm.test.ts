import assert from 'node:assert/strict';
import test from 'node:test';
import { buildAgentProfileInput, createAgentProfileDraft, createAgentProfileDraftFromProfile } from '../src/pages/agentProfiles/form.ts';
import type { AgentProfileView } from '../src/api/types.ts';

const profile: AgentProfileView = {
  profile_id: 'planner',
  version: 'v1',
  name: 'Planner',
  description: 'Plans work',
  supported_client_types: ['pi', 'claude_code'],
  agent_kind: 'planner',
  system_prompt_template: 'system',
  turn_prompt_template: 'turn',
  default_session_role: 'planner',
  default_session_description: 'planning session',
  handle_prefix: 'plan',
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

  assert.equal(draft.profile_id, 'planner');
  assert.equal(draft.version, '');
  assert.equal(draft.name, 'Planner');
  assert.equal(draft.supported_client_types_text, 'pi, claude_code');
  assert.equal(draft.agent_kind, 'planner');
  assert.match(draft.artifact_contract_text, /"kind": "patch"/);
});

test('builds an upsert input with parsed JSON fields and split client types', () => {
  const draft = createAgentProfileDraft();
  draft.profile_id = 'coder';
  draft.version = 'v1';
  draft.name = 'Coder';
  draft.supported_client_types_text = 'pi, claude_code, pi';
  draft.description = '';
  draft.artifact_contract_text = '{"outputs":["patch"]}';

  const result = buildAgentProfileInput(draft);

  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.deepEqual(result.input.supported_client_types, ['pi', 'claude_code']);
  assert.equal(result.input.agent_kind, 'executor');
  assert.equal(result.input.description, null);
  assert.deepEqual(result.input.artifact_contract, { outputs: ['patch'] });
});

test('returns field errors without building input when required or JSON fields are invalid', () => {
  const draft = createAgentProfileDraft();
  draft.version = 'v1';
  draft.name = 'Broken';
  draft.metadata_text = '{not-json';

  const result = buildAgentProfileInput(draft);

  assert.equal(result.ok, false);
  if (result.ok) return;
  assert.equal(result.errors.profile_id, 'Profile ID is required.');
  assert.match(result.errors.metadata_text ?? '', /Invalid JSON/);
});
