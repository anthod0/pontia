import { describe, expect, test } from 'vitest';
import { promptValueAfterEnter } from '../../src/lib/promptEnterBehavior';

describe('promptValueAfterEnter', () => {
  test('submits on plain Enter when the trimmed prompt does not end with backslash', () => {
    expect(promptValueAfterEnter('implement feature')).toBeNull();
  });

  test('deletes a trailing backslash and inserts a newline instead of submitting', () => {
    expect(promptValueAfterEnter('first line\\')).toBe('first line\n');
  });

  test('checks the trimmed prompt end and removes whitespace after the backslash', () => {
    expect(promptValueAfterEnter('first line\\   ')).toBe('first line\n');
  });

  test('checks the entire prompt end rather than the cursor line', () => {
    expect(promptValueAfterEnter('line with cursor\\\nfinal line')).toBeNull();
  });
});
