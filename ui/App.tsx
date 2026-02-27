import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { I18nContext, getStoredLang, translations, type AppLang, type TranslationKey } from "./i18n";
import { SettingsView } from "./components/SettingsView";
import { OverlayView } from "./components/OverlayView";
import { ToastProvider } from "./components/Toast";

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null);
  const [lang, setLangState] = useState<AppLang>(getStoredLang);

  const setLang = (l: AppLang) => {
    setLangState(l);
    localStorage.setItem("dictea_lang", l);
  };

  const t = (key: TranslationKey) => translations[lang][key];

  useEffect(() => {
    setWindowLabel(getCurrentWindow().label);
  }, []);

  if (windowLabel === null) {
    return null;
  }

  return (
    <I18nContext.Provider value={{ t, lang, setLang }}>
      <ToastProvider>
        {windowLabel === "overlay" ? <OverlayView /> : <SettingsView />}
      </ToastProvider>
    </I18nContext.Provider>
  );
}

export default App;
