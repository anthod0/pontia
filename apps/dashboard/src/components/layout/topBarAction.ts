import { getContext, setContext, type Snippet } from "svelte";
import { writable, type Writable } from "svelte/store";

const key = Symbol("top bar action");

export function provideTopBarAction() {
  return setContext(key, writable<Snippet | null>(null));
}

export function useTopBarAction() {
  return getContext<Writable<Snippet | null> | undefined>(key);
}
