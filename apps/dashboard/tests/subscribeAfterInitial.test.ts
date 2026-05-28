import { expect, test } from 'vitest';
import { writable } from 'svelte/store';
import { subscribeAfterInitial } from '../src/stores/subscribeAfterInitial.ts';

test('subscribeAfterInitial ignores the immediate store emission and handles later changes', () => {
  const store = writable('initial');
  const seen: string[] = [];

  const unsubscribe = subscribeAfterInitial(store, (value) => {
    seen.push(value);
  });

  expect(seen).toEqual([]);

  store.set('changed');
  expect(seen).toEqual(['changed']);

  unsubscribe();
  store.set('ignored');
  expect(seen).toEqual(['changed']);
});
