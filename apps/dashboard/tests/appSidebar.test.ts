import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { expect, test } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appSidebarSource = readFileSync(resolve(__dirname, '../src/components/layout/AppSidebar.svelte'), 'utf8');

test('clamps recent session titles in the dashboard sidebar to one line', () => {
  expect(appSidebarSource).toContain('<span class="line-clamp-1">{sessionChatTitle(session)}</span>');
});

test('shows a semantic status dot for non-exited sidebar sessions instead of a state badge', () => {
  expect(appSidebarSource).not.toContain('{session.state}</span>');
  expect(appSidebarSource).toContain('{#if isSessionVisibleState(session.state)}');
  expect(appSidebarSource).toContain('sessionStateDotClass(session.state)');
  expect(appSidebarSource).toContain("case 'busy':");
  expect(appSidebarSource).toContain('bg-blue-500');
  expect(appSidebarSource).toContain("case 'idle':");
  expect(appSidebarSource).toContain('bg-emerald-500');
  expect(appSidebarSource).toContain("return state !== 'exited'");
});

test('only marks New Chat active on the chat index route', () => {
  expect(appSidebarSource).toContain("if (path === '/chat') return currentPath === '/chat'");
  expect(appSidebarSource).not.toContain("if (path === '/chat') return currentPath === '/chat' || currentPath.startsWith('/chat/')");
});
