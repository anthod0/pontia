import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, test } from 'vitest';
import { capabilityRows, extraCapabilityRows } from '../../src/pages/sessions/sessionCapabilities';

const pageSource = readFileSync(join(process.cwd(), 'src/pages/SessionDetailPage.svelte'), 'utf8');

describe('SessionDetailPage presentation', () => {
  test('opens Messages first and exposes the conversation Tree before Details', () => {
    expect(pageSource).toContain('<Tabs.Root value="messages"');
    expect(pageSource.indexOf('<Tabs.Trigger value="messages">Messages</Tabs.Trigger>')).toBeLessThan(
      pageSource.indexOf('<Tabs.Trigger value="tree">Tree</Tabs.Trigger>'),
    );
    expect(pageSource.indexOf('<Tabs.Trigger value="tree">Tree</Tabs.Trigger>')).toBeLessThan(
      pageSource.indexOf('<Tabs.Trigger value="details">Details</Tabs.Trigger>'),
    );
    expect(pageSource).toContain('<SessionTurnTree turns={$sessionDetail.turns} currentTurnId={$sessionDetail.session.current_turn_id} />');
    expect(pageSource).not.toContain('<Tabs.Trigger value="overview">Overview</Tabs.Trigger>');
  });

  test('does not show removed explanatory copy or raw capabilities json', () => {
    expect(pageSource).not.toContain('Inspect and operate on one session. Use the sessions page for list browsing.');
    expect(pageSource).not.toContain('Advertised runtime behavior; unsupported actions are not faked.');
    expect(pageSource).not.toContain('JSON.stringify($sessionDetail.session.capabilities');
  });

  test('disables Open Chat when timeline is unsupported', () => {
    expect(pageSource).toContain("disabled={selectedSession?.capabilities?.timeline !== true}");
  });
});

describe('session capability presentation rows', () => {
  test('maps known boolean capabilities to visible support states', () => {
    expect(capabilityRows({ accept_task: true, interrupt: false, stream_output: true, heartbeat: false, timeline: true })).toEqual([
      { key: 'accept_task', label: 'Accept task', value: 'Supported', supported: true },
      { key: 'interrupt', label: 'Interrupt', value: 'Unsupported', supported: false },
      { key: 'stream_output', label: 'Stream output', value: 'Supported', supported: true },
      { key: 'heartbeat', label: 'Heartbeat', value: 'Unsupported', supported: false },
      { key: 'timeline', label: 'Timeline', value: 'Supported', supported: true },
      { key: 'context_usage', label: 'Context usage', value: 'Unsupported', supported: false },
    ]);
  });

  test('shows context usage mode and preserves unknown capability values as text rows', () => {
    expect(capabilityRows({ context_usage: 'estimated' }).at(-1)).toEqual({
      key: 'context_usage',
      label: 'Context usage',
      value: 'Estimated',
      supported: true,
    });
    expect(extraCapabilityRows({ accept_task: true, custom_mode: 'safe', nested: { level: 2 } })).toEqual([
      ['custom_mode', 'safe'],
      ['nested', '{"level":2}'],
    ]);
  });
});
