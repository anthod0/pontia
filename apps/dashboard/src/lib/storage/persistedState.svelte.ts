import { readStoredWith, writeStored } from "./webStorage";

export type Serializer<T> = {
  /** Returns undefined, or throws, for saved data that should fall back to the default. */
  parse(saved: string): T | undefined;
  /** Returns null to remove the key. */
  stringify(value: T): string | null;
};

/**
 * Reactive state saved in localStorage under a fixed key. It reads storage once, when created,
 * and does not follow writes from other tabs. Create a new instance when the key changes.
 * When storage is unavailable or full, the value lives in memory only.
 */
export class PersistedState<T> {
  #key: string;
  #serializer: Serializer<T>;
  #value: T;

  constructor(key: string, fallback: T, serializer: Serializer<T>) {
    this.#key = key;
    this.#serializer = serializer;
    // Raw state: a value is saved when it is replaced, so nested changes must not look saved.
    this.#value = $state.raw(readStoredWith(key, serializer.parse) ?? fallback);
  }

  get current(): T {
    return this.#value;
  }

  set current(value: T) {
    writeStored(this.#key, this.#serializer.stringify(value));
    this.#value = value;
  }
}

/** Saves a string as is and removes the key for null. */
export const textSerializer: Serializer<string | null> = {
  parse: (saved) => saved,
  stringify: (value) => value,
};
