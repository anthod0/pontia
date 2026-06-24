import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

describe('system theme styling', () => {
  test('uses native prefers-color-scheme CSS instead of runtime dark class setup', () => {
    const appCss = readFileSync(resolve(__dirname, '../../src/app.css'), 'utf8');
    const mainTs = readFileSync(resolve(__dirname, '../../src/main.ts'), 'utf8');

    expect(appCss).toContain('@media (prefers-color-scheme: dark)');
    expect(appCss).not.toContain('@custom-variant dark (&:is(.dark *));');
    expect(appCss).not.toMatch(/^\.dark\s*\{/m);
    expect(mainTs).not.toContain('initializeSystemTheme');
  });
});
