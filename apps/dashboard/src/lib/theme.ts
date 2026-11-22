import { derived, readable, toStore } from "svelte/store";
import { PersistedState } from "./storage";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

// Keep in sync with the inline script in app.html, which applies the theme before first paint.
const storageKey = "pontia.theme";
const themeColors: Record<ResolvedTheme, string> = { light: "#fafaf8", dark: "#1d2021" };
const darkQuery = "(prefers-color-scheme: dark)";

const preference = new PersistedState<ThemePreference>(storageKey, "system", {
  parse: (saved) => (saved === "light" || saved === "dark" ? saved : undefined),
  stringify: (value) => (value === "system" ? null : value),
});

const systemTheme = readable<ResolvedTheme>("light", (set) => {
  if (typeof matchMedia === "undefined") return;
  const query = matchMedia(darkQuery);
  const update = () => set(query.matches ? "dark" : "light");
  update();
  query.addEventListener("change", update);
  return () => query.removeEventListener("change", update);
});

export const themePreference = toStore(
  () => preference.current,
  (next) => {
    preference.current = next;
  },
);

export const resolvedTheme = derived([themePreference, systemTheme], ([$preference, $system]) =>
  $preference === "system" ? $system : $preference,
);

export function applyTheme(theme: ResolvedTheme): void {
  document.documentElement.classList.toggle("dark", theme === "dark");
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", themeColors[theme]);
}
