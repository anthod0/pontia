import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import assert from 'node:assert/strict';

const selector = readFileSync(new URL('./WorkspaceSelector.svelte', import.meta.url), 'utf8');
const createSessionForm = readFileSync(new URL('../sessions/CreateSessionForm.svelte', import.meta.url), 'utf8');
const createTaskForm = readFileSync(new URL('../tasks/CreateTaskForm.svelte', import.meta.url), 'utf8');
const taskDetail = readFileSync(new URL('../tasks/TaskDetail.svelte', import.meta.url), 'utf8');

test('workspace selector exposes known workspace selection and registration controls', () => {
  assert.match(selector, /export let selectedWorkspaceId/);
  assert.match(selector, /createEventDispatcher<\{\s*selected:/s);
  assert.match(selector, /loadWorkspaces\(\)/);
  assert.match(selector, /loadWorkspaceRoots\(\)/);
  assert.match(selector, /registerWorkspace\(\{/);
  assert.match(selector, /Register workspace/);
  assert.match(selector, /Register selected directory/);
});

test('create session form uses shared workspace selector and submits workspace_id', () => {
  assert.match(createSessionForm, /import WorkspaceSelector from '..\/workspaces\/WorkspaceSelector.svelte'/);
  assert.match(createSessionForm, /<WorkspaceSelector bind:selectedWorkspaceId=\{workspaceId\}/);
  assert.match(createSessionForm, /workspace_id:\s*workspaceId \|\| null/);
  assert.doesNotMatch(createSessionForm, /browseWorkspaceRoot|registerSelectedWorkspace|workspaceRoots/);
});

test('task forms use workspace selector while submitting canonical workspace paths', () => {
  assert.match(createTaskForm, /<WorkspaceSelector[\s\S]*on:selected=\{handleWorkspaceSelected\}/);
  assert.match(createTaskForm, /workspace:\s*workspacePath \|\| null/);
  assert.match(taskDetail, /<WorkspaceSelector[\s\S]*on:selected=\{handleWorkspaceSelected\}/);
  assert.match(taskDetail, /workspace:\s*workspacePath/);
});
