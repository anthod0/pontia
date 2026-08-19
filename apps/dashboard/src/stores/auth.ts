import { writable } from 'svelte/store';

const storageKey = 'pontia.externalApiToken';
const initialToken = typeof localStorage === 'undefined' ? '' : localStorage.getItem(storageKey) ?? '';

export const token = writable(initialToken);

token.subscribe((value) => {
  if (typeof localStorage !== 'undefined') localStorage.setItem(storageKey, value);
});

export function consumeTokenFromUrl(): void {
  if (typeof window === 'undefined') return;

  const url = new URL(window.location.href);
  if (!url.searchParams.has('token')) return;

  const urlToken = url.searchParams.get('token')?.trim() ?? '';
  if (urlToken) token.set(urlToken);

  url.searchParams.delete('token');
  window.history.replaceState(window.history.state, '', `${url.pathname}${url.search}${url.hash}`);
}
