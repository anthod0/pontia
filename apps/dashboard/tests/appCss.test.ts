import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, test } from 'vitest';

const appCss = readFileSync(resolve(__dirname, '../src/app.css'), 'utf8');

test('uses light syntax highlighting for rendered markdown code blocks', () => {
  expect(appCss).toContain("@import 'highlight.js/styles/github.css';");
  expect(appCss).not.toContain("@import 'highlight.js/styles/github-dark.css';");
});

test('defines subtle themed global scrollbar styling', () => {
  expect(appCss).toContain('scrollbar-width: thin');
  expect(appCss).toContain('scrollbar-color: var(--scrollbar-thumb) transparent');
  expect(appCss).toContain('::-webkit-scrollbar');
  expect(appCss).toContain('::-webkit-scrollbar-thumb:hover');
  expect(appCss).toContain('--scrollbar-thumb: color-mix(in oklch, var(--border) 75%, transparent)');
});
