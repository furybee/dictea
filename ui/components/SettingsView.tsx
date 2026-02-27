import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { Sidebar } from "./Sidebar";
import { DictationPage } from "./pages/DictationPage";
import { EnginePage } from "./pages/EnginePage";
import { ShortcutPage } from "./pages/ShortcutPage";
import { SettingsPage } from "./pages/SettingsPage";
import { useConfig } from "../hooks/useConfig";
import { useToast } from "./Toast";
import { PAGE_GLOW_COLORS, type Page } from "../types";

export function SettingsView() {
  const [activePage, setActivePage] = useState<Page>("dictation");
  const config = useConfig();
  const { showToast } = useToast();

  useEffect(() => {
    const unlisten = listen<string>("config_error", (event) => {
      showToast(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
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
              setApiKey={config.setApiKey}
              mistralApiKey={config.mistralApiKey}
              setMistralApiKey={config.setMistralApiKey}
              geminiApiKey={config.geminiApiKey}
              setGeminiApiKey={config.setGeminiApiKey}
              sttEngine={config.sttEngine}
              setSttEngine={config.setSttEngine}
            />
          )}

          {activePage === "shortcut" && <ShortcutPage />}

          {activePage === "settings" && <SettingsPage />}
        </div>
      </main>
    </div>
  );
}
