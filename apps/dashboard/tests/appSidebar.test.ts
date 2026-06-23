import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { expect, test } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appSidebarSource = readFileSync(resolve(__dirname, '../src/components/layout/AppSidebar.svelte'), 'utf8');
const sidebarMenuActionSource = readFileSync(resolve(__dirname, '../src/lib/components/ui/sidebar/sidebar-menu-action.svelte'), 'utf8');
const sessionMenuItemStart = appSidebarSource.indexOf('{#snippet sessionMenuItem(session: SessionView, actionKey: string)}');
const sessionMenuItemEnd = appSidebarSource.indexOf('<Sidebar.Root collapsible="icon">', sessionMenuItemStart);
const recentSessionItemSource = appSidebarSource.slice(sessionMenuItemStart, sessionMenuItemEnd);

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

test('places recent session status dots at the far right and swaps them for the hover actions menu', () => {
  expect(recentSessionItemSource).toContain('class="group-has-data-[sidebar=menu-action]/menu-item:pr-8"');
  expect(recentSessionItemSource).toContain('onclick={() => openSession(session.session_id)}>');
  expect(recentSessionItemSource).toContain('<span class="line-clamp-1">{sessionChatTitle(session)}</span>');
  expect(recentSessionItemSource).toContain("class={cn('pointer-events-none absolute top-1.5 right-1 flex aspect-square w-5 items-center justify-center transition-opacity group-focus-within/menu-item:opacity-0 group-hover/menu-item:opacity-0 group-data-[collapsible=icon]:hidden'");
  expect(recentSessionItemSource).toContain("sessionActionMenuOpenKey === actionKey ? 'opacity-0' : 'opacity-100'");
  expect(recentSessionItemSource).toContain('class={`size-2 rounded-full ${sessionStateDotClass(session.state)}`}');
  expect(recentSessionItemSource).toContain('<MoreHorizontal />');
  expect(recentSessionItemSource).toContain('<DropdownMenu.Item onclick={() => startRenamingSession(session)}>');
  expect(recentSessionItemSource).toContain('{#if session.pinned_at}');
  expect(recentSessionItemSource).toContain('<PinOff class="size-4" /> Unpin');
  expect(recentSessionItemSource).toContain('<Pin class="size-4" /> Pin');
  expect(recentSessionItemSource).toContain('onclick={() => void togglePinSessionFromMenu(session)}');
  expect(recentSessionItemSource).toContain('onclick={() => void archiveSessionFromMenu(session)}');
  expect(recentSessionItemSource).not.toContain('absolute right-2 top-1/2 size-2 -translate-y-1/2');
  expect(recentSessionItemSource).not.toContain('class="right-10"');
});


test('shows a pin icon before pinned recent session titles', () => {
  const pinIconIndex = recentSessionItemSource.indexOf('<Pin class="size-3.5 shrink-0 text-sidebar-foreground/60" aria-label="Pinned session" />');
  const titleIndex = recentSessionItemSource.indexOf('<span class="line-clamp-1">{sessionChatTitle(session)}</span>');

  expect(recentSessionItemSource).toContain('{#if session.pinned_at}');
  expect(pinIconIndex).toBeGreaterThan(-1);
  expect(titleIndex).toBeGreaterThan(-1);
  expect(pinIconIndex).toBeLessThan(titleIndex);
});

test('closes the recent session actions menu immediately after pin or archive is selected', () => {
  expect(appSidebarSource).toContain('let sessionActionMenuOpenKey = $state<string | null>(null)');
  expect(appSidebarSource).toContain('function setSessionActionMenuOpen(actionKey: string, open: boolean): void');
  expect(appSidebarSource).toContain('sessionActionMenuOpenKey = open ? actionKey : null');
  expect(appSidebarSource).toContain('async function togglePinSessionFromMenu(session: SessionView): Promise<void>');
  expect(appSidebarSource).toContain('sessionActionMenuOpenKey = null');
  expect(appSidebarSource).toContain('async function archiveSessionFromMenu(session: SessionView): Promise<void>');
  expect(appSidebarSource).toContain('bind:open={() => sessionActionMenuOpenKey === actionKey, (open) => setSessionActionMenuOpen(actionKey, open)}');
});

test('allows backend-limited recent sessions while scrolling recent workspace and session groups together', () => {
  expect(appSidebarSource).not.toContain('const recentSessionLimit = 50');
  expect(appSidebarSource).toContain("visibleChatSessions($sessions, 'all')");
  expect(appSidebarSource).not.toContain("visibleChatSessions($sessions, 'all').slice(0, recentSessionLimit)");
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
