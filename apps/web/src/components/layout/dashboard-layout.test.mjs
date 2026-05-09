import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const sidebar = readFileSync(new URL('./Sidebar.svelte', import.meta.url), 'utf8');
const appShell = readFileSync(new URL('./AppShell.svelte', import.meta.url), 'utf8');

test('dashboard uses session-first layout without task/planner panels', () => {
  assert.match(sidebar, /import CreateSessionForm from/);
  assert.match(sidebar, /<CreateSessionForm \/>/);
  assert.doesNotMatch(sidebar, /CreateTaskForm|TaskList|Compatibility: create session directly/);
  assert.doesNotMatch(sidebar, /<details class="panel">/);

  assert.doesNotMatch(appShell, /TaskDetail|tasks\/TaskDetail/);
});
