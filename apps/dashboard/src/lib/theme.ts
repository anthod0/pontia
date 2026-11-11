import { derived, readable, writable } from "svelte/store";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

// Keep in sync with the inline script in app.html, which applies the theme before first paint.
const storageKey = "pontia.theme";
const themeColors: Record<ResolvedTheme, string> = { light: "#fafaf8", dark: "#1d2021" };
const darkQuery = "(prefers-color-scheme: dark)";

function readPreference(): ThemePreference {
  if (typeof localStorage === "undefined") return "system";
  const saved = localStorage.getItem(storageKey);
  return saved === "light" || saved === "dark" ? saved : "system";
}

const preference = writable<ThemePreference>(readPreference());

const systemTheme = readable<ResolvedTheme>("light", (set) => {
  if (typeof matchMedia === "undefined") return;
  const query = matchMedia(darkQuery);
  const update = () => set(query.matches ? "dark" : "light");
  update();
  query.addEventListener("change", update);
  return () => query.removeEventListener("change", update);
});

export const themePreference = {
  subscribe: preference.subscribe,
  set(next: ThemePreference): void {
    if (next === "system") localStorage.removeItem(storageKey);
    else localStorage.setItem(storageKey, next);
    preference.set(next);
  },
};

export const resolvedTheme = derived([preference, systemTheme], ([$preference, $system]) =>
  $preference === "system" ? $system : $preference,
);

export function applyTheme(theme: ResolvedTheme): void {
  document.documentElement.classList.toggle("dark", theme === "dark");
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", themeColors[theme]);
}
