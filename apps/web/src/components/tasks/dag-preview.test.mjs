import { readFileSync } from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';

const taskDetail = readFileSync(new URL('./TaskDetail.svelte', import.meta.url), 'utf8');
const dagPreview = readFileSync(new URL('./DagPreview.svelte', import.meta.url), 'utf8');

test('task detail renders DAG through the DagPreview component', () => {
  assert.match(taskDetail, /import DagPreview from '\.\/DagPreview\.svelte'/);
  assert.match(taskDetail, /<DagPreview dag=\{\$taskDag\}/);
});

test('dag preview exposes dependency and next-node context for each work item', () => {
  assert.match(dagPreview, /directDependencies/);
  assert.match(dagPreview, /directDependents/);
  assert.match(dagPreview, /Depends on/);
  assert.match(dagPreview, /Next/);
  assert.match(dagPreview, /statusClass/);
});
