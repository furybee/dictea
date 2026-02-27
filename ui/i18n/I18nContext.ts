import { createContext, useContext } from "react";
import { translations, type TranslationKey, type AppLang } from "./translations";

interface I18nValue {
  t: (key: TranslationKey) => string;
  lang: AppLang;
  setLang: (l: AppLang) => void;
}

export const I18nContext = createContext<I18nValue>({
  t: (key) => translations.en[key],
  lang: "en",
  setLang: () => {},
});

export function useI18n() {
  return useContext(I18nContext);
}

export function getStoredLang(): AppLang {
  const stored = localStorage.getItem("dictea_lang");
  if (stored === "en" || stored === "fr") return stored;
  return "en";
}
