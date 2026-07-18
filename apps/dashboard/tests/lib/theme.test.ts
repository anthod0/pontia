import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

describe('system theme styling', () => {
  test('uses native prefers-color-scheme CSS instead of runtime dark class setup', () => {
    const appCss = readFileSync(resolve(__dirname, '../../src/app.css'), 'utf8');
    const layout = readFileSync(resolve(__dirname, '../../src/routes/+layout.svelte'), 'utf8');

    expect(appCss).toContain('@media (prefers-color-scheme: dark)');
    expect(appCss).not.toContain('@custom-variant dark (&:is(.dark *));');
    expect(appCss).not.toMatch(/^\.dark\s*\{/m);
    expect(layout).not.toContain('initializeSystemTheme');
  });

  test('loads a dark highlight.js theme when the system is in dark mode', () => {
    const appCss = readFileSync(resolve(__dirname, '../../src/app.css'), 'utf8');

    expect(appCss).toContain("@import 'highlight.js/styles/github.css';");
    expect(appCss).toContain("@import 'highlight.js/styles/github-dark-dimmed.css' (prefers-color-scheme: dark);");
  });
});
