/** URL prefix of each translated locale. English, the base locale, is served without a prefix. */
export const localePrefixes = { "zh-Hans": "zh", ja: "ja", "pt-BR": "pt-br" } as const;

/** Pages that exist only in English and keep the same URL in every locale. */
export const englishOnlyPages = ["/privacy", "/terms"] as const;

/** Language names shown in the language switcher, each written in its own language. */
export const localeNames = {
  en: "English",
  "zh-Hans": "简体中文",
  ja: "日本語",
  "pt-BR": "Português",
} as const;

/** `hreflang` value of each locale. */
export const hreflangs = { en: "en", "zh-Hans": "zh-Hans", ja: "ja", "pt-BR": "pt-BR" } as const;

/** Open Graph `og:locale` value of each locale. */
export const ogLocales = {
  en: "en_US",
  "zh-Hans": "zh_CN",
  ja: "ja_JP",
  "pt-BR": "pt_BR",
} as const;
