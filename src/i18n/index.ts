import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";
import ru from "./ru.json";

export const SUPPORTED_LANGUAGES = ["en", "ru"] as const;
export type Language = (typeof SUPPORTED_LANGUAGES)[number];

/** Explicit setting wins; otherwise follow the OS language; otherwise English. */
export function resolveLanguage(setting: string | null): Language {
  if (setting === "en" || setting === "ru") return setting;
  const os = typeof navigator !== "undefined" ? navigator.language.toLowerCase() : "en";
  return os.startsWith("ru") ? "ru" : "en";
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, ru: { translation: ru } },
  lng: resolveLanguage(null),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;
