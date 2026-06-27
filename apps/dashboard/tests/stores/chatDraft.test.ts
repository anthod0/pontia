import { get } from 'svelte/store';
import { describe, expect, test } from 'vitest';
import { chatDraft, clearChatDraft, setChatDraft } from '../../src/stores/chatDraft';

describe('chat draft store', () => {
  test('keeps one in-memory draft shared by chat pages until it is cleared', () => {
    setChatDraft('inspect the auth flow');

    expect(get(chatDraft)).toBe('inspect the auth flow');

    setChatDraft('continue with tests');
    expect(get(chatDraft)).toBe('continue with tests');

    clearChatDraft();
    expect(get(chatDraft)).toBe('');
  });
});
