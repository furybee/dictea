import type { TranslationKey } from "./i18n";

export interface AppConfig {
  global_shortcut: string;
  openai_api_key: string;
  output_language: string;
  reformulate: boolean;
  stt_engine: string;
  mistral_api_key: string;
  gemini_api_key: string;
}

export type Page = "dictation" | "engine" | "shortcut" | "settings";

export const PAGE_GLOW_COLORS: Record<Page, string> = {
  dictation: "99, 102, 241",
  engine: "245, 158, 11",
  shortcut: "16, 185, 129",
  settings: "139, 92, 246",
};

export const OUTPUT_LANGUAGES: { code: string; labelKey?: TranslationKey; label?: string }[] = [
  { code: "auto", labelKey: "lang_auto" },
  { code: "fr", label: "Francais" },
  { code: "en", label: "English" },
  { code: "es", label: "Espanol" },
  { code: "de", label: "Deutsch" },
  { code: "it", label: "Italiano" },
  { code: "pt", label: "Portugues" },
];
