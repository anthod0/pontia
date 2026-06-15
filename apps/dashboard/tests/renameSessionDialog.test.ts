import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { expect, test } from 'vitest';

const __dirname = dirname(fileURLToPath(import.meta.url));
const chatPageSource = readFileSync(resolve(__dirname, '../src/pages/ChatPage.svelte'), 'utf8');
const dialogSource = readFileSync(resolve(__dirname, '../src/components/chat/RenameSessionDialog.svelte'), 'utf8');

test('chat page uses the shared dialog instead of window.prompt for renaming sessions', () => {
  expect(chatPageSource).toContain("import RenameSessionDialog from '../components/chat/RenameSessionDialog.svelte'");
  expect(chatPageSource).toContain('<RenameSessionDialog');
  expect(chatPageSource).not.toContain('window.prompt');
});

test('rename session dialog is built from shadcn dialog primitives', () => {
  expect(dialogSource).toContain("import * as Dialog from '$lib/components/ui/dialog/index.js'");
  expect(dialogSource).toContain('<Dialog.Root');
  expect(dialogSource).toContain('<Dialog.Content');
  expect(dialogSource).toContain('<Dialog.Title>Rename session</Dialog.Title>');
});
