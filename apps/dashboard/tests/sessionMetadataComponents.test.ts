import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { Check, Loader, LogOut, Pause, TriangleAlert } from '@lucide/svelte';
import { describe, expect, test } from 'vitest';
import { sessionStateBadgeClass, sessionStateIcon, sessionStateIconClass } from '../src/components/chat/sessionMetadata';

const componentPath = (...parts: string[]) => resolve(__dirname, '../src/components/chat', ...parts);

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

  test('composer dock uses the compact session metadata component for all viewports', () => {
    const source = readFileSync(componentPath('SessionComposerDock.svelte'), 'utf8');

    expect(source).not.toContain("import SessionMetadataDesktop from './SessionMetadataDesktop.svelte'");
    expect(source).toContain("import SessionMetadataMobile from './SessionMetadataMobile.svelte'");
    expect(source).not.toContain('data-testid="session-status-desktop-metadata"');
    expect(source).not.toContain('data-testid="session-status-mobile-metadata" class="relative min-w-0 flex-1 sm:hidden"');
    expect(source).not.toContain('SessionMetadataBadges');
  });

  test('composer dock uses the component-library dropdown menu instead of a hand-rolled floating menu', () => {
    const source = readFileSync(componentPath('SessionComposerDock.svelte'), 'utf8');

    expect(source).toContain("import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'");
    expect(source).not.toContain("import { tick } from 'svelte'");
    expect(source).not.toContain('getBoundingClientRect');
    expect(source).not.toContain('advancedControlsMenuEl');
    expect(source).not.toContain('data-placement={advancedControlsPlacement}');
  });

  test('mobile metadata details use the component-library popover', () => {
    const source = readFileSync(componentPath('SessionMetadataMobile.svelte'), 'utf8');

    expect(source).toContain("import * as Popover from '$lib/components/ui/popover/index.js'");
    expect(source).not.toContain('absolute bottom-full');
    expect(source).not.toContain('{#if sessionDetailsOpen}');
  });

  test('git inline status does not color the branch name', () => {
    const source = readFileSync(componentPath('GitStatusInline.svelte'), 'utf8');

    expect(source).not.toContain('gitStatusToneClass');
    expect(source).toContain('<span>{gitBranchLabel(gitStatus)}</span>');
  });

  test('session metadata trigger inlines desktop-only icons', () => {
    const source = readFileSync(componentPath('SessionMetadataMobile.svelte'), 'utf8');
    const triggerSource = source.slice(source.indexOf('<Popover.Trigger'), source.indexOf('</Popover.Trigger>'));

    expect(triggerSource).toContain('<Folder class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" />');
    expect(triggerSource).toContain('<GitBranch class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" />');
    expect(triggerSource).toContain('<Gauge class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" />');
    expect(triggerSource).toContain('<Terminal class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" />');
    expect(triggerSource).toContain('<Bot class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" />');
    expect(triggerSource).toContain('<AtSign class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" />');
  });
});
