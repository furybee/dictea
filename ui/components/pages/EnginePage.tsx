import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../../i18n";

interface EnginePageProps {
  apiKey: string;
  setApiKey: (v: string) => void;
  mistralApiKey: string;
  setMistralApiKey: (v: string) => void;
  geminiApiKey: string;
  setGeminiApiKey: (v: string) => void;
  groqApiKey: string;
  setGroqApiKey: (v: string) => void;
  sttEngine: string;
  setSttEngine: (v: string) => void;
  parakeetReformProvider: string;
  setParakeetReformProvider: (v: string) => void;
}

interface ParakeetModelStatus {
  downloaded: boolean;
  downloading: boolean;
  path: string;
}

interface DownloadProgress {
  file: string;
  file_index: number;
  file_count: number;
  downloaded: number;
  total: number;
}

function formatMB(bytes: number): string {
  return (bytes / (1024 * 1024)).toFixed(0);
}

function ParakeetModelSection() {
  const { t } = useI18n();
  const [status, setStatus] = useState<ParakeetModelStatus | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<ParakeetModelStatus>("parakeet_model_status")
      .then(setStatus)
      .catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
    const unlistenProgress = listen<DownloadProgress>("parakeet_download_progress", (e) => {
      setProgress(e.payload);
    });
    const unlistenDone = listen("parakeet_download_done", () => {
      setProgress(null);
      refresh();
    });
    const unlistenError = listen<string>("parakeet_download_error", (e) => {
      setProgress(null);
      setError(e.payload);
      refresh();
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenDone.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [refresh]);

  const startDownload = () => {
    setError(null);
    setStatus((s) => (s ? { ...s, downloading: true } : s));
    invoke("download_parakeet_model").catch((e) => {
      setError(String(e));
      refresh();
    });
  };

  const deleteModel = () => {
    invoke("delete_parakeet_model")
      .then(refresh)
      .catch((e) => setError(String(e)));
  };

  const percent =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : 0;

  return (
    <div className="settings-section">
      <h2>{t("parakeet_model")}</h2>
      <p className="hint">{t("parakeet_model_hint")}</p>

      {status?.downloading ? (
        <div className="model-download">
          <div className="model-status-row">
            <span className="model-status-text">
              {t("downloading_model")} ({progress ? `${progress.file_index}/${progress.file_count}` : "..."})
              {progress && progress.total > 0 && (
                <> — {formatMB(progress.downloaded)} / {formatMB(progress.total)} Mo</>
              )}
            </span>
          </div>
          <div className="progress-track">
            <div className="progress-fill" style={{ width: `${percent}%` }} />
          </div>
        </div>
      ) : status?.downloaded ? (
        <div className="model-status-row">
          <span className="model-status-badge model-status-ok">✓ {t("model_downloaded")}</span>
          <button className="btn-secondary" onClick={deleteModel}>
            {t("delete_model")}
          </button>
        </div>
      ) : (
        <div className="model-status-row">
          <span className="model-status-text">{t("model_not_downloaded")}</span>
          <button className="btn-primary" onClick={startDownload}>
            {t("download_model")}
          </button>
        </div>
      )}

      {error && (
        <p className="model-error">
          {t("download_error")}: {error}
        </p>
      )}
    </div>
  );
}

export function EnginePage({
  apiKey,
  setApiKey,
  mistralApiKey,
  setMistralApiKey,
  geminiApiKey,
  setGeminiApiKey,
  groqApiKey,
  setGroqApiKey,
  sttEngine,
  setSttEngine,
  parakeetReformProvider,
  setParakeetReformProvider,
}: EnginePageProps) {
  const { t } = useI18n();

  const engineConfig: Record<string, {
    label: string; hint: string; key: string;
    setKey: (v: string) => void; placeholder: string;
    transcription: string; reformulation: string;
  }> = {
    openai: {
      label: t("api_key"), hint: t("api_key_hint"),
      key: apiKey, setKey: setApiKey, placeholder: "sk-...",
      transcription: "gpt-4o-transcribe", reformulation: "gpt-4o-mini",
    },
    voxtral: {
      label: t("api_key_mistral"), hint: t("api_key_mistral_hint"),
      key: mistralApiKey, setKey: setMistralApiKey, placeholder: "",
      transcription: "voxtral-mini-latest", reformulation: "mistral-small-latest",
    },
    gemini: {
      label: t("api_key_gemini"), hint: t("api_key_gemini_hint"),
      key: geminiApiKey, setKey: setGeminiApiKey, placeholder: "",
      transcription: "gemini-2.5-flash", reformulation: "gemini-2.5-flash-lite",
    },
    groq: {
      label: t("api_key_groq"), hint: t("api_key_groq_hint"),
      key: groqApiKey, setKey: setGroqApiKey, placeholder: "gsk_...",
      transcription: "whisper-large-v3-turbo", reformulation: "llama-3.3-70b-versatile",
    },
  };

  const isParakeet = sttEngine === "parakeet";
  const current = engineConfig[sttEngine] || engineConfig.openai;
  const reformProvider = engineConfig[parakeetReformProvider] || engineConfig.openai;

  return (
    <>
      <h2 className="page-title">{t("page_engine")}</h2>

      <div className="settings-section">
        <h2>{t("stt_engine")}</h2>
        <p className="hint">{t("stt_engine_hint")}</p>
        <select
          className="settings-select"
          value={sttEngine}
          onChange={(e) => setSttEngine(e.target.value)}
        >
          <option value="openai">{t("openai_api")}</option>
          <option value="groq">{t("groq_api")}</option>
          <option value="voxtral">{t("voxtral_api")}</option>
          <option value="gemini">{t("gemini_api")}</option>
          <option value="parakeet">{t("parakeet_api")}</option>
        </select>
      </div>

      {isParakeet ? (
        <>
          <ParakeetModelSection />

          <div className="settings-section">
            <h2>{t("reformulation_provider")}</h2>
            <p className="hint">{t("parakeet_reformulate_hint")}</p>
            <select
              className="settings-select"
              value={parakeetReformProvider}
              onChange={(e) => setParakeetReformProvider(e.target.value)}
            >
              <option value="openai">OpenAI</option>
              <option value="groq">Groq</option>
              <option value="voxtral">Mistral</option>
              <option value="gemini">Gemini (Google)</option>
            </select>
          </div>

          <div className="settings-section">
            <h2>{reformProvider.label}</h2>
            <p className="hint">{reformProvider.hint}</p>
            <input
              type="password"
              className="settings-input"
              value={reformProvider.key}
              onChange={(e) => reformProvider.setKey(e.target.value)}
              placeholder={reformProvider.placeholder}
            />
          </div>

          <div className="settings-section">
            <h2>{t("models_used")}</h2>
            <div className="models-list">
              <div className="model-item">
                <span className="model-label">{t("model_transcription")}</span>
                <code className="model-name">parakeet-tdt-0.6b-v3 (local)</code>
              </div>
              <div className="model-item">
                <span className="model-label">{t("model_reformulation")}</span>
                <code className="model-name">{reformProvider.reformulation}</code>
              </div>
            </div>
          </div>
        </>
      ) : (
        <>
          <div className="settings-section">
            <h2>{current.label}</h2>
            <p className="hint">{current.hint}</p>
            <input
              type="password"
              className="settings-input"
              value={current.key}
              onChange={(e) => current.setKey(e.target.value)}
              placeholder={current.placeholder}
            />
          </div>

          <div className="settings-section">
            <h2>{t("models_used")}</h2>
            <p className="hint">{t("models_used_hint")}</p>
            <div className="models-list">
              <div className="model-item">
                <span className="model-label">{t("model_transcription")}</span>
                <code className="model-name">{current.transcription}</code>
              </div>
              <div className="model-item">
                <span className="model-label">{t("model_reformulation")}</span>
                <code className="model-name">{current.reformulation}</code>
              </div>
            </div>
          </div>
        </>
      )}
    </>
  );
}
