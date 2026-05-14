import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const source = readFileSync(new URL('./WorkspacesPage.svelte', import.meta.url), 'utf8');

test('directory rows open by clicking the underlined folder name', () => {
  assert.match(source, /<button[^>]+aria-label="Open directory \{entry\.name\}"[^>]+onclick=\{\(\) => void openPath\(entry\.path\)\}/s);
  assert.match(source, /class="[^"]*hover:underline[^"]*"/);
  assert.match(source, /\{entry\.name\}\//);
});

test('directory rows do not render a separate Open action button', () => {
  assert.doesNotMatch(source, /<Button[^>]+aria-label="Open directory \{entry\.name\}"/s);
  assert.doesNotMatch(source, />\s*Open\s*<FolderOpen class="size-4"/s);
});
