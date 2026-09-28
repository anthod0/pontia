import type { Readable, Unsubscriber } from "svelte/store";

export function subscribeAfterInitial<T>(
  store: Readable<T>,
  run: (value: T) => void,
): Unsubscriber {
  let initial = true;
  return store.subscribe((value) => {
    if (initial) {
      initial = false;
      return;
    }
    run(value);
  });
}
