import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const source = readFileSync(new URL('../src/pages/TasksPage.svelte', import.meta.url), 'utf8');

test('DAG tasks table uses fixed-width columns inside its scroll container', () => {
  assert.match(source, /<Table\.Root class="table-fixed"/, 'table should use fixed layout so long task fields do not expand the page');
  assert.match(source, /<Table\.Head class="w-\[45%\]">Task<\/Table\.Head>/, 'task column should own the flexible text width');
});

test('DAG tasks table clips long workspace identifiers', () => {
  assert.match(source, /<Table\.Cell class="max-w-0">\s*<div class="truncate" title=\{item\.workspace_id \?\? '—'\}>\{item\.workspace_id \?\? '—'\}<\/div>\s*<\/Table\.Cell>/s);
});
