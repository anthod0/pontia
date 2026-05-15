import type { AgentProfileView, JsonObject, UpsertAgentProfileInput } from '../../api/types';

export interface AgentProfileDraft {
  profile_id: string;
  version: string;
  name: string;
  description: string;
  supported_client_types_text: string;
  system_prompt_template: string;
  turn_prompt_template: string;
  default_session_role: string;
  default_session_description: string;
  handle_prefix: string;
  expected_output_schema: string;
  artifact_contract_text: string;
  default_execution_policy_text: string;
  default_review_policy_text: string;
  metadata_text: string;
}

export type AgentProfileDraftErrors = Partial<Record<keyof AgentProfileDraft, string>>;

export type BuildAgentProfileInputResult =
  | { ok: true; input: UpsertAgentProfileInput }
  | { ok: false; errors: AgentProfileDraftErrors };

const EMPTY_JSON = '{}';

export function createAgentProfileDraft(): AgentProfileDraft {
  return {
    profile_id: '',
    version: '',
    name: '',
    description: '',
    supported_client_types_text: 'pi, claude_code',
    system_prompt_template: '',
    turn_prompt_template: '',
    default_session_role: '',
    default_session_description: '',
    handle_prefix: '',
    expected_output_schema: '',
    artifact_contract_text: EMPTY_JSON,
    default_execution_policy_text: EMPTY_JSON,
    default_review_policy_text: EMPTY_JSON,
    metadata_text: EMPTY_JSON,
  };
}

export function createAgentProfileDraftFromProfile(
  profile: AgentProfileView,
  options: { clearVersion?: boolean } = {},
): AgentProfileDraft {
  return {
    profile_id: profile.profile_id,
    version: options.clearVersion ? '' : profile.version,
    name: profile.name,
    description: profile.description ?? '',
    supported_client_types_text: profile.supported_client_types.join(', '),
    system_prompt_template: profile.system_prompt_template ?? '',
    turn_prompt_template: profile.turn_prompt_template ?? '',
    default_session_role: profile.default_session_role ?? '',
    default_session_description: profile.default_session_description ?? '',
    handle_prefix: profile.handle_prefix ?? '',
    expected_output_schema: profile.expected_output_schema ?? '',
    artifact_contract_text: stringifyJson(profile.artifact_contract),
    default_execution_policy_text: stringifyJson(profile.default_execution_policy),
    default_review_policy_text: stringifyJson(profile.default_review_policy),
    metadata_text: stringifyJson(profile.metadata),
  };
}

export function buildAgentProfileInput(draft: AgentProfileDraft): BuildAgentProfileInputResult {
  const errors: AgentProfileDraftErrors = {};

  if (!draft.profile_id.trim()) errors.profile_id = 'Profile ID is required.';
  if (!draft.version.trim()) errors.version = 'Version is required.';
  if (!draft.name.trim()) errors.name = 'Name is required.';

  const artifactContract = parseJsonObject('artifact_contract_text', draft.artifact_contract_text, errors);
  const executionPolicy = parseJsonObject('default_execution_policy_text', draft.default_execution_policy_text, errors);
  const reviewPolicy = parseJsonObject('default_review_policy_text', draft.default_review_policy_text, errors);
  const metadata = parseJsonObject('metadata_text', draft.metadata_text, errors);

  if (Object.keys(errors).length > 0) return { ok: false, errors };

  return {
    ok: true,
    input: {
      profile_id: draft.profile_id.trim(),
      version: draft.version.trim(),
      name: draft.name.trim(),
      description: nullableTrimmed(draft.description),
      supported_client_types: splitClientTypes(draft.supported_client_types_text),
      system_prompt_template: nullableTrimmed(draft.system_prompt_template),
      turn_prompt_template: nullableTrimmed(draft.turn_prompt_template),
      default_session_role: nullableTrimmed(draft.default_session_role),
      default_session_description: nullableTrimmed(draft.default_session_description),
      handle_prefix: nullableTrimmed(draft.handle_prefix),
      expected_output_schema: nullableTrimmed(draft.expected_output_schema),
      artifact_contract: artifactContract,
      default_execution_policy: executionPolicy,
      default_review_policy: reviewPolicy,
      metadata,
    },
  };
}

function nullableTrimmed(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function splitClientTypes(value: string): string[] {
  return Array.from(new Set(value.split(/[\s,]+/).map((client) => client.trim()).filter(Boolean)));
}

function stringifyJson(value: unknown): string {
  return JSON.stringify(value ?? {}, null, 2);
}

function parseJsonObject(
  field: keyof AgentProfileDraft,
  value: string,
  errors: AgentProfileDraftErrors,
): JsonObject {
  const source = value.trim() || EMPTY_JSON;
  try {
    const parsed = JSON.parse(source) as unknown;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      errors[field] = 'Value must be a JSON object.';
      return {};
    }
    return parsed as JsonObject;
  } catch (error) {
    errors[field] = `Invalid JSON: ${error instanceof Error ? error.message : String(error)}`;
    return {};
  }
}
