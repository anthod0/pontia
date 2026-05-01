import { writable } from 'svelte/store';

export const statusMessage = writable('Enter an External API token, then load or create a session.');
export const statusError = writable(false);

export function setStatus(message: string, error = false): void {
  statusMessage.set(message);
  statusError.set(error);
}
