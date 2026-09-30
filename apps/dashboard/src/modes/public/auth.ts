import { writable } from "svelte/store";

export const token = writable("");

export function loadTokenFromStorage(): string {
  return "";
}

export function consumeTokenFromUrl(): void {}
