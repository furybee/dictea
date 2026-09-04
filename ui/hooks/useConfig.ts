import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppConfig } from "../types";

export function useConfig() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [mistralApiKey, setMistralApiKey] = useState("");
  const [geminiApiKey, setGeminiApiKey] = useState("");
  const [groqApiKey, setGroqApiKey] = useState("");
  const [audioDevice, setAudioDevice] = useState("");
  const [sttEngine, setSttEngine] = useState("openai");
  const [reformProvider, setReformProvider] = useState("auto");
  const [outputLanguage, setOutputLanguage] = useState("auto");
  const [reformulate, setReformulate] = useState(false);
  const loaded = useRef(false);

  useEffect(() => {
    const load = async () => {
      try {
        const appConfig = await invoke<AppConfig>("get_config");
        setConfig(appConfig);
        setReformulate(appConfig.reformulate);
        setApiKey(appConfig.openai_api_key);
        setMistralApiKey(appConfig.mistral_api_key);
        setGeminiApiKey(appConfig.gemini_api_key);
        setGroqApiKey(appConfig.groq_api_key || "");
        setAudioDevice(appConfig.audio_device || "");
        setSttEngine(appConfig.stt_engine || "openai");
        setReformProvider(appConfig.reformulation_provider || "auto");
        setOutputLanguage(appConfig.output_language);
        loaded.current = true;
      } catch (e) {
        console.error(e);
      }
    };
    load();
  }, []);

  const autoSave = useCallback(() => {
    if (!config || !loaded.current) return;
    invoke("set_config", {
      config: {
        ...config,
        openai_api_key: apiKey,
        mistral_api_key: mistralApiKey,
        gemini_api_key: geminiApiKey,
        groq_api_key: groqApiKey,
        audio_device: audioDevice,
        stt_engine: sttEngine,
        output_language: outputLanguage,
        reformulate,
        reformulation_provider: reformProvider,
      },
    }).catch(console.error);
  }, [config, apiKey, mistralApiKey, geminiApiKey, groqApiKey, audioDevice, sttEngine, outputLanguage, reformulate, reformProvider]);

  useEffect(() => {
    if (!loaded.current) return;
    const timer = setTimeout(autoSave, 400);
    return () => clearTimeout(timer);
  }, [autoSave]);

  return {
    apiKey,
    setApiKey,
    mistralApiKey,
    setMistralApiKey,
    geminiApiKey,
    setGeminiApiKey,
    groqApiKey,
    setGroqApiKey,
    audioDevice,
    setAudioDevice,
    sttEngine,
    setSttEngine,
    reformProvider,
    setReformProvider,
    outputLanguage,
    setOutputLanguage,
    reformulate,
    setReformulate,
  };
}
