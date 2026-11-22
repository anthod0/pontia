import { derived, get, toStore } from "svelte/store";
import { addMessages, init, locale, format } from "svelte-i18n";
import en from "./en.json";
import zhCN from "./zh-CN.json";
import { PersistedState } from "../storage";

export type Locale = "en" | "zh-CN";
export type LanguagePreference = "system" | Locale;
export type MessageKey = keyof typeof en;
export type Translator = (
  key: MessageKey,
  values?: Record<string, string | number | boolean | Date | null | undefined>,
) => string;

const storageKey = "pontia.locale";

const preference = new PersistedState<LanguagePreference>(storageKey, "system", {
  parse: (saved) => (saved === "en" || saved === "zh-CN" ? saved : undefined),
  stringify: (value) => (value === "system" ? null : value),
});

function browserLocale(): Locale {
  for (const language of typeof navigator === "undefined" ? [] : navigator.languages) {
    if (/^zh(?:-|$)/i.test(language)) return "zh-CN";
    if (/^en(?:-|$)/i.test(language)) return "en";
  }
  return "en";
}

addMessages("en", en);
addMessages("zh-CN", zhCN);
init({ fallbackLocale: "en", initialLocale: resolveLocale(preference.current) });

function resolveLocale(value: LanguagePreference): Locale {
  return value === "system" ? browserLocale() : value;
}

export const languagePreference = toStore(
  () => preference.current,
  (value) => {
    preference.current = value;
    locale.set(resolveLocale(value));
  },
);

export function updateBrowserLanguage(): void {
  if (preference.current === "system") locale.set(browserLocale());
}

export const currentLocale = derived(locale, (value) => value as Locale);
export const t = derived(
  format,
  ($format): Translator =>
    (key, values) =>
      $format(key, { values }),
);

export const translate: Translator = (key, values) => get(t)(key, values);

export { date, time, number } from "svelte-i18n";
