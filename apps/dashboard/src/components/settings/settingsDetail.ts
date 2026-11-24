import { getContext, setContext } from "svelte";
import { writable, type Writable } from "svelte/store";

/** An item a settings page opened from its list. Phones title the top bar with it and go back to the list. */
export type SettingsDetail = {
  title: string;
  back: () => void;
  disabled: boolean;
};

const key = Symbol("settings detail");

export function provideSettingsDetail() {
  return setContext(key, writable<SettingsDetail | null>(null));
}

export function useSettingsDetail() {
  return getContext<Writable<SettingsDetail | null> | undefined>(key);
}
