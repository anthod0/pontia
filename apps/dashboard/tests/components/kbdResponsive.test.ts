import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, test } from 'vitest';

const kbdSource = readFileSync(resolve(__dirname, '../../src/lib/components/ui/kbd/kbd.svelte'), 'utf8');
const kbdGroupSource = readFileSync(resolve(__dirname, '../../src/lib/components/ui/kbd/kbd-group.svelte'), 'utf8');

test('kbd components are hidden on mobile and shown from the sm breakpoint', () => {
  expect(kbdSource).toContain('hidden sm:inline-flex');
  expect(kbdGroupSource).toContain('hidden sm:inline-flex');
});
