import { PersistedState } from "$lib/storage";
import {
  comboKeys,
  overlaps,
  recordError,
  registry,
  sameCombo,
  shortcutIds,
  type KeyCombo,
  type ShortcutId,
} from "./registry";
import { formatShortcutKeys } from "./format";

type Bindings = Partial<Record<ShortcutId, KeyCombo | null>>;
type Preferences = { version: 1; bindings: Bindings };
const key = "pontia.shortcuts";
function readPreferences() {
  return new PersistedState<Preferences>(
    key,
    { version: 1, bindings: {} },
    {
      parse(saved) {
        const data = JSON.parse(saved);
        if (data?.version !== 1 || !data.bindings || typeof data.bindings !== "object") return;
        const bindings: Bindings = {};
        for (const id of Object.keys(registry) as ShortcutId[]) {
          const value = data.bindings[id];
          if (value === null) bindings[id] = null;
          else if (
            id !== "openChatByPosition" &&
            typeof value?.code === "string" &&
            ["mod", "alt", "shift", "secondary"].every(
              (k) => value[k] === undefined || typeof value[k] === "boolean",
            ) &&
            !recordError(id, value) &&
            !sameCombo(value, registry[id].combo)
          )
            bindings[id] = value;
        }
        return { version: 1, bindings };
      },
      stringify: (value) => (Object.keys(value.bindings).length ? JSON.stringify(value) : null),
    },
  );
}

class ShortcutStore {
  #preferences = $state.raw(readPreferences());

  sync = () => {
    this.#preferences = readPreferences();
    const reload = (event: StorageEvent) => {
      if ((event.key === key || event.key === null) && event.storageArea === window.localStorage)
        this.#preferences = readPreferences();
    };
    window.addEventListener("storage", reload);
    return () => window.removeEventListener("storage", reload);
  };

  binding(id: ShortcutId) {
    const value = this.#preferences.current.bindings[id];
    return value === undefined ? registry[id].combo : value;
  }
  label(id: ShortcutId) {
    const value = this.binding(id);
    return value ? formatShortcutKeys(comboKeys(value)) : "";
  }
  changed(id: ShortcutId) {
    return this.#preferences.current.bindings[id] !== undefined;
  }
  conflicts(id: ShortcutId, combo: KeyCombo) {
    return shortcutIds.filter((other) => {
      const binding = this.binding(other);
      return other !== id && binding && overlaps(id, combo, other, binding);
    });
  }
  assign(id: ShortcutId, combo: KeyCombo | null, replace: ShortcutId[] = []) {
    const bindings = { ...this.#preferences.current.bindings };
    for (const other of replace) bindings[other] = null;
    if (combo && sameCombo(combo, registry[id].combo)) delete bindings[id];
    else bindings[id] = combo;
    this.#preferences.current = { version: 1, bindings };
  }
  reset() {
    this.#preferences.current = { version: 1, bindings: {} };
  }
}

export const shortcuts = new ShortcutStore();
