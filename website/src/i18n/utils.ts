import { defaultLang, type Lang } from "./locales";

/** Per-locale string dictionary: one entry per language, same keys in each. */
export type Dictionary<T> = Record<Lang, T>;

export function useTranslations<T extends object>(
  dict: Dictionary<T>,
  lang: Lang,
) {
  return function t<K extends keyof T>(key: K): T[K] {
    return dict[lang][key] ?? dict[defaultLang][key];
  };
}
