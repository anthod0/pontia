import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../src/pages/WorkspacesPage.svelte', import.meta.url), 'utf8');

test('workspace page removes the separate register current directory panel', () => {
  assert.doesNotMatch(source, /<Card\.Title>Register current directory<\/Card\.Title>/, 'separate register current directory panel should be removed');
  assert.doesNotMatch(source, /registerCurrentWorkspace/, 'registration should flow through Active actions instead of a separate panel action');
});

test('workspace panels use a responsive side-by-side layout with browser on the left', () => {
  assert.match(source, /<div class="grid gap-6 xl:grid-cols-\[minmax\(0,1\.1fr\)_minmax\(0,0\.9fr\)\] xl:items-start">/);
  assert.match(source, /<div class="xl:order-1">\s*<Card\.Root>\s*<Card\.Header>\s*<Card\.Title class="flex items-center gap-2"><FolderOpen class="size-5" \/> Root browser<\/Card\.Title>/s);
  assert.match(source, /<div class="xl:order-2">\s*<Card\.Root>\s*<Card\.Header><Card\.Title>Active workspaces<\/Card\.Title>/s);
});


test('active workspaces expose a rename action', () => {
  assert.match(source, /aria-label=\{`Rename \$\{workspaceLabel\}`\}/);
  assert.match(source, /onclick=\{\(\) => startRenamingWorkspace\(workspace\)\}/);
  assert.match(source, /Confirm workspace rename/);
});
