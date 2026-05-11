import {
  clientTypeOptionsForProfile,
  defaultHandleForProfile,
  metadataForProfile,
  selectClientTypeForProfile,
} from './agentProfiles';
import type { AgentProfileView } from '../api/types';

function profile(overrides: Partial<AgentProfileView> = {}): AgentProfileView {
  return {
    profile_id: 'reviewer',
    version: '1',
    name: 'Reviewer',
    description: 'Reviews work',
    supported_client_types: ['pi'],
    system_prompt_template: null,
    turn_prompt_template: null,
    default_session_role: 'Reviewer',
    default_session_description: 'Reviews assigned work.',
    handle_prefix: 'reviewer',
    session_reuse_policy: 'fresh_per_work_item',
    expected_output_schema: 'review_result_v1',
    artifact_contract: {},
    default_execution_policy: {},
    default_review_policy: {},
    metadata: {},
    created_at: '2026-05-11T00:00:00Z',
    updated_at: '2026-05-11T00:00:00Z',
    ...overrides,
  };
}

function assertEqual(actual: unknown, expected: unknown, label: string): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

assertEqual(clientTypeOptionsForProfile(null), ['claude_code', 'pi', 'generic'], 'fallback clients');
assertEqual(clientTypeOptionsForProfile(profile()), ['pi'], 'profile-supported clients');
assertEqual(selectClientTypeForProfile('claude_code', profile()), 'pi', 'unsupported current client replaced');
assertEqual(selectClientTypeForProfile('pi', profile()), 'pi', 'supported current client kept');
assertEqual(defaultHandleForProfile(profile()), '@reviewer', 'handle prefix converted to valid handle');
assertEqual(defaultHandleForProfile(profile({ handle_prefix: '@qa' })), '@qa', 'at-prefixed handle kept');
assertEqual(metadataForProfile(profile()), { profile_id: 'reviewer', profile_version: '1' }, 'profile metadata');
