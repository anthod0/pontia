import type { SessionCapabilities } from '../../api/types';

export interface CapabilityRow {
  key: string;
  label: string;
  value: string;
  supported: boolean;
}

const BOOLEAN_CAPABILITIES: Array<[keyof SessionCapabilities, string]> = [
  ['accept_task', 'Accept task'],
  ['interrupt', 'Interrupt'],
  ['stream_output', 'Stream output'],
  ['heartbeat', 'Heartbeat'],
  ['timeline', 'Timeline'],
];

const KNOWN_CAPABILITY_KEYS = new Set<string>([
  ...BOOLEAN_CAPABILITIES.map(([key]) => String(key)),
  'context_usage',
]);

function formatCapabilityValue(value: unknown): string {
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  if (value === null || value === undefined) return '—';
  return JSON.stringify(value);
}

function titleCase(value: string): string {
  return value.replaceAll('_', ' ').replace(/^./, (letter) => letter.toUpperCase());
}

export function capabilityRows(capabilities: SessionCapabilities | null | undefined): CapabilityRow[] {
  const rows = BOOLEAN_CAPABILITIES.map(([key, label]) => {
    const supported = capabilities?.[key] === true;
    return {
      key: String(key),
      label,
      value: supported ? 'Supported' : 'Unsupported',
      supported,
    };
  });

  const contextUsage = capabilities?.context_usage ?? 'unsupported';
  rows.push({
    key: 'context_usage',
    label: 'Context usage',
    value: titleCase(contextUsage),
    supported: contextUsage !== 'unsupported',
  });

  return rows;
}

export function extraCapabilityRows(capabilities: SessionCapabilities | null | undefined): Array<[string, string]> {
  return Object.entries(capabilities ?? {})
    .filter(([key]) => !KNOWN_CAPABILITY_KEYS.has(key))
    .map(([key, value]) => [key, formatCapabilityValue(value)]);
}
