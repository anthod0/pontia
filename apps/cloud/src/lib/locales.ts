export const localeInfo = {
  en: { prefix: "", name: "English", og: "en_US" },
  "zh-Hans": { prefix: "zh", name: "简体中文", og: "zh_CN" },
  ja: { prefix: "ja", name: "日本語", og: "ja_JP" },
  "pt-BR": { prefix: "pt-br", name: "Português", og: "pt_BR" },
} as const;

/** Pages that exist only in English and keep the same URL in every locale. */
export const englishOnlyPages = ["/privacy", "/terms"] as const;
