import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const componentPath = (...parts: string[]) => resolve(__dirname, '../src/components/chat', ...parts);

describe('session metadata component boundaries', () => {
  test('composer dock uses viewport-specific session metadata components', () => {
    const source = readFileSync(componentPath('SessionComposerDock.svelte'), 'utf8');

    expect(source).toContain("import SessionMetadataDesktop from './SessionMetadataDesktop.svelte'");
    expect(source).toContain("import SessionMetadataMobile from './SessionMetadataMobile.svelte'");
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
});
