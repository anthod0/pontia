import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const indexHtml = readFileSync(resolve(__dirname, '../index.html'), 'utf8');
const appSidebarSource = readFileSync(resolve(__dirname, '../src/components/layout/AppSidebar.svelte'), 'utf8');

test('dashboard head advertises packaged logo icons', () => {
  expect(indexHtml).toContain('<link rel="icon" type="image/svg+xml" href="/dashboard/logo.svg" />');
  expect(indexHtml).toContain('<link rel="icon" href="/dashboard/favicon.ico" sizes="any" />');
  expect(indexHtml).toContain('<link rel="icon" type="image/png" sizes="32x32" href="/dashboard/logo-32.png" />');
  expect(indexHtml).toContain('<link rel="icon" type="image/png" sizes="192x192" href="/dashboard/logo-192.png" />');
  expect(indexHtml).toContain('<link rel="apple-touch-icon" sizes="180x180" href="/dashboard/logo-180.png" />');
});

test('dashboard sidebar uses the SVG logo asset', () => {
  expect(appSidebarSource).toContain('src="/dashboard/logo.svg"');
  expect(appSidebarSource).toContain('class="size-6 shrink-0 object-contain"');
  expect(appSidebarSource).toContain('alt=""');
  expect(appSidebarSource).not.toContain('bg-sidebar-primary');
  expect(appSidebarSource).not.toContain('>ll</span>');
});

test('dashboard sidebar keeps the logo visible when collapsed', () => {
  expect(appSidebarSource).toContain('group-data-[collapsible=icon]:size-8');
  expect(appSidebarSource).toContain('group-data-[collapsible=icon]:p-0');
  expect(appSidebarSource).toContain('shrink-0');
  expect(appSidebarSource).toContain('group-data-[collapsible=icon]:hidden">pilotfy Dashboard</span>');
});
