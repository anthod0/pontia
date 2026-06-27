import { writable } from 'svelte/store';

export const chatDraft = writable('');

export function setChatDraft(value: string): void {
  chatDraft.set(value);
}

export function clearChatDraft(): void {
  chatDraft.set('');
}
