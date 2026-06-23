import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { expect, test } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appSidebarSource = readFileSync(resolve(__dirname, '../src/components/layout/AppSidebar.svelte'), 'utf8');
const sidebarMenuActionSource = readFileSync(resolve(__dirname, '../src/lib/components/ui/sidebar/sidebar-menu-action.svelte'), 'utf8');

test('clamps recent session titles in the dashboard sidebar to one line', () => {
  expect(appSidebarSource).toContain('<span class="line-clamp-1">{sessionChatTitle(session)}</span>');
});

test('shows a semantic status dot for non-exited sidebar sessions instead of a state badge', () => {
  expect(appSidebarSource).not.toContain('{session.state}</span>');
  expect(appSidebarSource).toContain('{#if isSessionVisibleState(session.state)}');
  expect(appSidebarSource).toContain('sessionStateDotClass(session.state)');
  expect(appSidebarSource).toContain("case 'busy':");
  expect(appSidebarSource).toContain('bg-amber-500');
  expect(appSidebarSource).not.toContain('bg-blue-500');
  expect(appSidebarSource).toContain("case 'idle':");
  expect(appSidebarSource).toContain("case 'interrupted':");
  expect(appSidebarSource).toContain('bg-emerald-500');
  expect(appSidebarSource).toContain("return state !== 'exited'");
});

test('places recent session status dots before titles and the hover actions menu at the far right', () => {
  expect(appSidebarSource).toContain('class="group-has-data-[sidebar=menu-action]/menu-item:pr-8"');
  expect(appSidebarSource).toContain('class={`size-2 shrink-0 rounded-full ${sessionStateDotClass(session.state)} group-data-[collapsible=icon]:hidden`}');
  expect(appSidebarSource).toContain('<MoreHorizontal />');
  expect(appSidebarSource).toContain('<DropdownMenu.Item onclick={() => startRenamingSession(session)}>');
  expect(appSidebarSource).toContain('{#if session.pinned_at}');
  expect(appSidebarSource).toContain('<PinOff class="size-4" /> Unpin');
  expect(appSidebarSource).toContain('<Pin class="size-4" /> Pin');
  expect(appSidebarSource).toContain('onclick={() => void togglePinSession(session)}');
  expect(appSidebarSource).toContain('onclick={() => void archiveSessionFromSidebar(session)}');
  expect(appSidebarSource).not.toContain('absolute right-2 top-1/2 size-2 -translate-y-1/2');
  expect(appSidebarSource).not.toContain('class="right-10"');
  expect(appSidebarSource).not.toContain('group-hover/menu-item:opacity-0');
  expect(appSidebarSource).not.toContain('group-focus-within/menu-item:opacity-0');
});

test('allows more recent sessions while scrolling recent workspace and session groups together', () => {
  expect(appSidebarSource).toContain('const recentSessionLimit = 50');
  expect(appSidebarSource).toContain("visibleChatSessions($sessions, 'all').slice(0, recentSessionLimit)");
  expect(appSidebarSource).toContain('<Sidebar.Content class="overflow-hidden">');
  expect(appSidebarSource).toContain('<div class="no-scrollbar min-h-0 flex-1 overflow-y-auto group-data-[collapsible=icon]:hidden">');
  expect(appSidebarSource).toContain('<Sidebar.GroupContent class="pr-1">');
  expect(appSidebarSource).not.toContain('<Sidebar.GroupContent class="no-scrollbar min-h-0 overflow-y-auto pr-1">');
});

test('hides the recent workspace and session groups when the sidebar is collapsed to icons', () => {
  expect(appSidebarSource).toContain('<div class="no-scrollbar min-h-0 flex-1 overflow-y-auto group-data-[collapsible=icon]:hidden">');
  expect(appSidebarSource).not.toContain('<div class="no-scrollbar min-h-0 flex-1 overflow-y-auto">');
});

test('lets the Recent Sessions header toggle the session list when the sidebar is expanded', () => {
  expect(appSidebarSource).toContain('let recentSessionsOpen = $state(true)');
  expect(appSidebarSource).toContain('aria-expanded={recentSessionsOpen}');
  expect(appSidebarSource).toContain('onclick={() => (recentSessionsOpen = !recentSessionsOpen)}');
  expect(appSidebarSource).toContain('{#if recentSessionsOpen}');
  expect(appSidebarSource).toContain('<Sidebar.GroupLabel class="p-0">');
  expect(appSidebarSource).toContain('Recent Sessions');
});

test('only marks New Chat active on the chat index route', () => {
  expect(appSidebarSource).toContain("if (path === '/chat') return currentPath === '/chat'");
  expect(appSidebarSource).not.toContain("if (path === '/chat') return currentPath === '/chat' || currentPath.startsWith('/chat/')");
});

test('hides hover-only sidebar menu actions by default on mobile and desktop', () => {
  expect(sidebarMenuActionSource).toContain('group-focus-within/menu-item:opacity-100 group-hover/menu-item:opacity-100 data-open:opacity-100 opacity-0');
  expect(sidebarMenuActionSource).not.toContain('data-open:opacity-100 md:opacity-0');
});

test('opens the settings menu above the trigger to avoid mobile viewport overflow', () => {
  expect(appSidebarSource).toContain('<DropdownMenu.Content side="top" align="end" class="w-48">');
  expect(appSidebarSource).not.toContain('<DropdownMenu.Content side="right" align="end" class="w-48">');
});

test('uses the shared dialog instead of window.prompt for renaming sessions', () => {
  expect(appSidebarSource).toContain("import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'");
  expect(appSidebarSource).toContain('<RenameSessionDialog');
  expect(appSidebarSource).not.toContain('window.prompt');
});
