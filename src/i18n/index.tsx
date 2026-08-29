import { createContext, createSignal, JSX, useContext } from "solid-js";
import { en } from "./locales/en";
import { zh } from "./locales/zh";

export type Locale = "en-US" | "zh-CN";

const dictionaries = {
  "en-US": en,
  "zh-CN": zh,
};

interface I18nContextValue {
  locale: () => Locale;
  setLocale: (l: Locale) => void;
  t: (path: string) => string;
}

const I18nContext = createContext<I18nContextValue>();

export function I18nProvider(props: { children: JSX.Element; defaultLocale?: Locale }) {
  const [locale, setLocale] = createSignal<Locale>(props.defaultLocale || "en-US");

  const t = (path: string): string => {
    const keys = path.split(".");
    let current: any = dictionaries[locale()] || en;
    for (const key of keys) {
      if (current && typeof current === "object" && key in current) {
        current = current[key];
      } else {
        // Fallback to en
        let fallback: any = en;
        for (const fbKey of keys) {
          if (fallback && typeof fallback === "object" && fbKey in fallback) {
            fallback = fallback[fbKey];
          } else {
            return path;
          }
        }
        return typeof fallback === "string" ? fallback : path;
      }
    }
    return typeof current === "string" ? current : path;
  };

  return (
    <I18nContext.Provider value={{ locale, setLocale, t }}>
      {props.children}
    </I18nContext.Provider>
  );
}

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error("useI18n must be used within an I18nProvider");
  }
  return ctx;
}
