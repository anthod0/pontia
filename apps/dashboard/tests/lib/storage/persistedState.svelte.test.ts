import { flushSync } from "svelte";
import { beforeEach, expect, test } from "vitest";
import { PersistedState, textSerializer } from "../../../src/lib/storage";

beforeEach(() => {
  localStorage.clear();
});

test("effects and derived values follow replacements", () => {
  const state = new PersistedState("pontia.text", null, textSerializer);
  const seen: (string | null)[] = [];
  const cleanup = $effect.root(() => {
    const upper = $derived(state.current?.toUpperCase() ?? null);
    $effect(() => {
      seen.push(upper);
    });
  });
  flushSync();
  state.current = "a";
  flushSync();
  state.current = null;
  flushSync();
  cleanup();
  expect(seen).toEqual([null, "A", null]);
});
