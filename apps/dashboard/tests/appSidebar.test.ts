import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { expect, test } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appSidebarSource = readFileSync(resolve(__dirname, '../src/components/layout/AppSidebar.svelte'), 'utf8');

test('clamps recent session titles in the dashboard sidebar to one line', () => {
  expect(appSidebarSource).toContain('<span class="line-clamp-1">{sessionChatTitle(session)}</span>');
});

test('shows a simple green dot for active sidebar sessions instead of a state badge', () => {
  expect(appSidebarSource).not.toContain('{session.state}</span>');
  expect(appSidebarSource).toContain('{#if isSessionActiveState(session.state)}');
  expect(appSidebarSource).toContain('bg-green-500');
});
