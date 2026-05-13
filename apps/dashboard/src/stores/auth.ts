import { writable } from 'svelte/store';

const storageKey = 'llmparty.externalApiToken';
const initialToken = typeof localStorage === 'undefined' ? '' : localStorage.getItem(storageKey) ?? '';

export const token = writable(initialToken);

token.subscribe((value) => {
  if (typeof localStorage !== 'undefined') localStorage.setItem(storageKey, value);
});
