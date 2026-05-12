import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const sidebar = readFileSync(new URL('./Sidebar.svelte', import.meta.url), 'utf8');
const appShell = readFileSync(new URL('./AppShell.svelte', import.meta.url), 'utf8');
const statusBar = readFileSync(new URL('./StatusBar.svelte', import.meta.url), 'utf8');
const globalCss = readFileSync(new URL('../../styles/global.css', import.meta.url), 'utf8');

test('dashboard exposes separate sessions and tasks DAG views', () => {
  assert.match(appShell, /dashboardView/);
  assert.match(appShell, /value="sessions"/);
  assert.match(appShell, /value="tasks"/);
  assert.match(appShell, /Tasks \/ DAG/);

  assert.match(sidebar, /import CreateTaskForm from/);
  assert.match(sidebar, /import TaskList from/);
  assert.match(sidebar, /{#if view === 'tasks'}/);
  assert.match(sidebar, /<CreateTaskForm \/>/);
  assert.match(sidebar, /<TaskList \/>/);
  assert.match(sidebar, /{:else}/);
  assert.match(sidebar, /<CreateSessionForm \/>/);

  assert.match(appShell, /import TaskDetail from/);
  assert.match(appShell, /{#if dashboardView === 'tasks'}/);
  assert.match(appShell, /<TaskDetail \/>/);
  assert.match(appShell, /{:else}/);
  assert.match(appShell, /<SessionDetail \/>/);
});

test('status bar keeps the API token input interactive under long status text', () => {
  assert.match(statusBar, /<label class="token-field">/);
  assert.match(statusBar, /<input[^>]+aria-label="External API token"/);
  assert.match(globalCss, /\.token-field\s*\{[^}]*flex:\s*0\s+0\s+min\(18rem,\s*100%\)/s);
  assert.match(globalCss, /\.status-stack\s*\{[^}]*min-width:\s*0/s);
  assert.match(globalCss, /\.status-stack\s+(?:span|small)\s*\{[^}]*overflow-wrap:\s*anywhere/s);
});

test('status bar API token input uses readable text color instead of inheriting the dark header color', () => {
  assert.match(globalCss, /\.token-field input\s*\{[^}]*color:\s*var\(--text\)/s);
});
