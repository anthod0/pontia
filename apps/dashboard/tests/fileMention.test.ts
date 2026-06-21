import { describe, expect, test } from 'vitest';
import { activeFileMention, replaceFileMention } from '../src/lib/file-picker/fileMention';

describe('file mention prompt editing', () => {
  test('detects an active @ token before the cursor', () => {
    expect(activeFileMention('please inspect @src/ma and report', 22)).toEqual({ start: 15, end: 22, query: 'src/ma' });
    expect(activeFileMention('email test@example.com', 18)).toBeNull();
  });

  test('replaces only the active @ token with the selected file path', () => {
    expect(replaceFileMention('open @src/ma please', { start: 5, end: 12, query: 'src/ma' }, 'src/main.rs')).toEqual({
      value: 'open @src/main.rs please',
      cursor: 17,
    });
  });
});
