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

test('uses small viewport height for app roots to avoid mobile browser chrome overflow', () => {
  expect(appCss).toContain('min-height: 100svh');
  expect(appCss).not.toContain('min-height: 100vh');
});

test('defines a reusable no-scrollbar utility for scrollable sidebar regions', () => {
  expect(appCss).toContain('.no-scrollbar {');
  expect(appCss).toContain('scrollbar-width: none');
  expect(appCss).toContain('-ms-overflow-style: none');
  expect(appCss).toContain('.no-scrollbar::-webkit-scrollbar');
  expect(appCss).toContain('display: none');
});
