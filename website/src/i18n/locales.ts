export const languages = {
  en: "English",
  ja: "日本語",
  zh: "中文",
} as const;

export type Lang = keyof typeof languages;

export const defaultLang: Lang = "en";
