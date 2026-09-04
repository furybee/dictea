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
  reformProvider: string;
  setReformProvider: (v: string) => void;
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

interface AppleStatus {
  availability: string;
  message: string;
}

function AppleIntelligenceSection({ status }: { status: AppleStatus | null }) {
  const { t } = useI18n();
  const ready = status?.availability === "available";

  return (
    <div className="settings-section">
      <h2>{t("apple_intelligence")}</h2>
      <p className="hint">{t("apple_intelligence_hint")}</p>
      <div className="model-status-row">
        <span className={`model-status-badge${ready ? " model-status-ok" : " model-status-warn"}`}>
          {ready ? "✓" : "!"} {status?.message ?? "…"}
        </span>
      </div>
      {!ready && <p className="hint">{t("apple_unavailable_note")}</p>}
    </div>
  );
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
    const unlistenCancelled = listen("parakeet_download_cancelled", () => {
      setProgress(null);
      refresh();
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenDone.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenCancelled.then((fn) => fn());
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

  const cancelDownload = () => {
    invoke("cancel_parakeet_download").catch(console.error);
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
            <button className="btn-secondary" onClick={cancelDownload}>
              {t("cancel_download")}
            </button>
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
  reformProvider,
  setReformProvider,
}: EnginePageProps) {
  const { t } = useI18n();
  const [appleStatus, setAppleStatus] = useState<AppleStatus | null>(null);

  useEffect(() => {
    invoke<AppleStatus>("apple_intelligence_status")
      .then(setAppleStatus)
      .catch(console.error);
  }, []);

  // Hide the option entirely off macOS rather than offer something that
  // can never work there
  const appleSupported = appleStatus !== null && appleStatus.availability !== "unsupported_os";

  const engineConfig: Record<string, {
    label: string; hint: string; key: string;
    setKey: (v: string) => void; placeholder: string;
    transcription: string; reformulation: string;
  }> = {
    openai: {
      label: t("api_key"), hint: t("api_key_hint"),
      key: apiKey, setKey: setApiKey, placeholder: "sk-...",
      transcription: "gpt-transcribe", reformulation: "gpt-4o-mini",
    },
    voxtral: {
      label: t("api_key_mistral"), hint: t("api_key_mistral_hint"),
      key: mistralApiKey, setKey: setMistralApiKey, placeholder: "",
      transcription: "voxtral-mini-2602", reformulation: "mistral-small-latest",
    },
    gemini: {
      label: t("api_key_gemini"), hint: t("api_key_gemini_hint"),
      key: geminiApiKey, setKey: setGeminiApiKey, placeholder: "",
      transcription: "gemini-3.5-transcribe", reformulation: "gemini-2.5-flash-lite",
    },
    groq: {
      label: t("api_key_groq"), hint: t("api_key_groq_hint"),
      key: groqApiKey, setKey: setGroqApiKey, placeholder: "gsk_...",
      transcription: "whisper-large-v3-turbo", reformulation: "llama-3.3-70b-versatile",
    },
  };

  const isParakeet = sttEngine === "parakeet";
  const current = engineConfig[sttEngine] || engineConfig.openai;

  // Mirrors resolve_reformulation_provider on the Rust side: auto follows the
  // STT engine, and the local engine cannot rewrite its own output
  const resolvedProvider =
    reformProvider === "auto" ? (isParakeet ? "openai" : sttEngine) : reformProvider;
  const isApple = resolvedProvider === "apple";
  const reformConfig = engineConfig[resolvedProvider] || engineConfig.openai;

  // Its key is already asked for above when it is also the STT engine
  const needsOwnKey = !isApple && resolvedProvider !== sttEngine;

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

      {/* The local engine has a model to fetch, the others a key to enter */}
      {isParakeet ? (
        <ParakeetModelSection />
      ) : (
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
      )}

      <div className="settings-section">
        <h2>{t("reformulation_provider")}</h2>
        <p className="hint">{t("reformulation_provider_hint")}</p>
        <select
          className="settings-select"
          value={reformProvider}
          onChange={(e) => setReformProvider(e.target.value)}
        >
          <option value="auto">{t("reformulation_auto")}</option>
          {appleSupported && <option value="apple">{t("apple_intelligence")}</option>}
          <option value="openai">OpenAI</option>
          <option value="groq">Groq</option>
          <option value="voxtral">Mistral</option>
          <option value="gemini">Gemini (Google)</option>
        </select>
      </div>

      {isApple && <AppleIntelligenceSection status={appleStatus} />}

      {/* A provider that is not the STT engine needs its own key */}
      {needsOwnKey && (
        <div className="settings-section">
          <h2>{reformConfig.label}</h2>
          <p className="hint">{reformConfig.hint}</p>
          <input
            type="password"
            className="settings-input"
            value={reformConfig.key}
            onChange={(e) => reformConfig.setKey(e.target.value)}
            placeholder={reformConfig.placeholder}
          />
        </div>
      )}

      <div className="settings-section">
        <h2>{t("models_used")}</h2>
        <div className="models-list">
          <div className="model-item">
            <span className="model-label">{t("model_transcription")}</span>
            <code className="model-name">
              {isParakeet ? "parakeet-tdt-0.6b-v3 (local)" : current.transcription}
            </code>
          </div>
          <div className="model-item">
            <span className="model-label">{t("model_reformulation")}</span>
            <code className="model-name">
              {isApple ? "apple on-device (local)" : reformConfig.reformulation}
            </code>
          </div>
        </div>
      </div>
    </>
  );
}
