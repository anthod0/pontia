import type { EventView, JsonObject } from '../../api/types';
import { shortId } from '../../components/tasks/format';

export function sessionEventSummary(payload: JsonObject | null | undefined): string {
  if (!payload || Object.keys(payload).length === 0) return 'No payload details';

  const primaryParts = [
    importantText(payload, ['summary', 'message']),
    nestedText(payload, ['input', 'summary']),
    nestedText(payload, ['output', 'summary']),
    nestedText(payload, ['error', 'message']),
    nestedText(payload, ['failure', 'message']),
  ].filter(Boolean) as string[];

  if (primaryParts.length === 0) {
    const fallback = Object.entries(payload).slice(0, 4).map(([key, value]) => `${key}=${compactValue(value)}`);
    return fallback.length ? fallback.join(' · ') : 'No payload details';
  }

  const parts = [...primaryParts];
  const reason = primitiveText(payload.reason);
  if (reason) parts.push(`reason=${reason}`);

  const state = primitiveText(payload.state);
  if (state) parts.push(`state=${state}`);

  return parts.slice(0, 4).join(' · ');
}

export function sessionEventDetailRows(event: Pick<EventView, 'event_id' | 'session_id' | 'turn_id' | 'source'>): Array<[string, string]> {
  return [
    ['Event ID', event.event_id],
    ['Session ID', event.session_id],
    ['Turn ID', event.turn_id ?? '—'],
    ['Source', event.source],
  ];
}

export function sessionEventTurnLabel(turnId: string | null | undefined): string {
  return turnId ? shortId(turnId) : '—';
}

function importantText(payload: JsonObject, keys: string[]): string | null {
  for (const key of keys) {
    const text = primitiveText(payload[key]);
    if (text) return key === 'summary' || key === 'message' ? text : `${key}=${text}`;
  }
  return null;
}

function nestedText(payload: JsonObject, path: string[]): string | null {
  let current: unknown = payload;
  for (const key of path) {
    if (!isRecord(current)) return null;
    current = current[key];
  }
  return primitiveText(current);
}

function primitiveText(value: unknown): string | null {
  if (typeof value === 'string' && value.trim()) return value.trim();
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return null;
}

function compactValue(value: unknown): string {
  const primitive = primitiveText(value);
  if (primitive) return primitive;
  if (value === null || value === undefined) return '—';
  if (Array.isArray(value)) return `[${value.length}]`;
  if (isRecord(value)) return '{…}';
  return String(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value));
}
