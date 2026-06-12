import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { expect, test } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const topBarSource = readFileSync(resolve(__dirname, '../src/components/layout/TopBar.svelte'), 'utf8');

test('shows SSE connection status on mobile with a compact label', () => {
  expect(topBarSource).toContain("const compactStatusLabel: Record<string, string>");
  expect(topBarSource).toContain("open: 'Live'");
  expect(topBarSource).toContain('class="inline-flex gap-1 md:hidden"');
  expect(topBarSource).toContain('{compactStatusLabel[$sseStatus] ?? $sseStatus}');
});

test('keeps full SSE connection status visible on desktop', () => {
  expect(topBarSource).toContain('class="hidden gap-1 md:inline-flex"');
  expect(topBarSource).toContain('{statusLabel[$sseStatus] ?? $sseStatus}');
});
