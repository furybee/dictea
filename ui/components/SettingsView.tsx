import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "../i18n";
import { Sidebar } from "./Sidebar";
import { DictationPage } from "./pages/DictationPage";
import { EnginePage } from "./pages/EnginePage";
import { ProvidersPage } from "./pages/ProvidersPage";
import { ShortcutPage } from "./pages/ShortcutPage";
import { SettingsPage } from "./pages/SettingsPage";
import { useConfig } from "../hooks/useConfig";
import { useToast } from "./Toast";
import { PAGE_GLOW_COLORS, type Page } from "../types";

export function SettingsView() {
  const [activePage, setActivePage] = useState<Page>("dictation");
  const [pasteFailed, setPasteFailed] = useState(false);
  const config = useConfig();
  const { showToast } = useToast();
  const { t } = useI18n();

  useEffect(() => {
    const unlistenConfig = listen<string>("config_error", (event) => {
      showToast(event.payload);
    });
    const unlistenStt = listen<string>("stt_error", (event) => {
      showToast(event.payload);
    });
    // Needs an action from the user, so a 3s toast will not do
    const unlistenPaste = listen("paste_failed", () => {
      setPasteFailed(true);
    });
    return () => {
      unlistenConfig.then((fn) => fn());
      unlistenStt.then((fn) => fn());
      unlistenPaste.then((fn) => fn());
    };
  }, [showToast]);

  return (
    <div className="app-layout">
      <div className="titlebar-drag" data-tauri-drag-region></div>
      <Sidebar activePage={activePage} onPageChange={setActivePage} />

      <main
        className="main-content"
        style={{ "--glow-color": PAGE_GLOW_COLORS[activePage] } as React.CSSProperties}
      >
        <div className="bg-blob bg-blob-1" />
        <div className="bg-blob bg-blob-2" />
        <div className="main-inner">
          {pasteFailed && (
            <div className="alert-banner">
              <div className="alert-banner-text">
                <strong>{t("paste_failed_title")}</strong>
                <p>{t("paste_failed_body")}</p>
              </div>
              <div className="alert-banner-actions">
                <button
                  className="btn-primary"
                  onClick={() => invoke("open_accessibility_settings").catch(console.error)}
                >
                  {t("paste_failed_action")}
                </button>
                <button className="btn-secondary" onClick={() => setPasteFailed(false)}>
                  {t("dismiss")}
                </button>
              </div>
            </div>
          )}
          {activePage === "dictation" && (
            <DictationPage
              outputLanguage={config.outputLanguage}
              setOutputLanguage={config.setOutputLanguage}
              reformulate={config.reformulate}
              setReformulate={config.setReformulate}
            />
          )}

          {activePage === "engine" && (
            <EnginePage
              apiKey={config.apiKey}
              mistralApiKey={config.mistralApiKey}
              geminiApiKey={config.geminiApiKey}
              groqApiKey={config.groqApiKey}
              sttEngine={config.sttEngine}
              setSttEngine={config.setSttEngine}
              reformProvider={config.reformProvider}
              setReformProvider={config.setReformProvider}
              onGoToProviders={() => setActivePage("providers")}
            />
          )}

          {activePage === "providers" && (
            <ProvidersPage
              apiKey={config.apiKey}
              setApiKey={config.setApiKey}
              mistralApiKey={config.mistralApiKey}
              setMistralApiKey={config.setMistralApiKey}
              geminiApiKey={config.geminiApiKey}
              setGeminiApiKey={config.setGeminiApiKey}
              groqApiKey={config.groqApiKey}
              setGroqApiKey={config.setGroqApiKey}
            />
          )}

          {activePage === "shortcut" && <ShortcutPage />}

          {activePage === "settings" && (
            <SettingsPage
              audioDevice={config.audioDevice}
              setAudioDevice={config.setAudioDevice}
            />
          )}
        </div>
      </main>
    </div>
  );
}
