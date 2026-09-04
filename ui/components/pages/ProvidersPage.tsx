import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../../i18n";
import { useLocalProviders, type AppleStatus } from "../../hooks/useLocalProviders";

interface ProvidersPageProps {
  apiKey: string;
  setApiKey: (v: string) => void;
  mistralApiKey: string;
  setMistralApiKey: (v: string) => void;
  geminiApiKey: string;
  setGeminiApiKey: (v: string) => void;
  groqApiKey: string;
  setGroqApiKey: (v: string) => void;
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

type TestState = "idle" | "testing" | "ok" | "failed";

/// One API provider: its key, and whether that key actually works.
function ApiProvider({
  name,
  usage,
  provider,
  value,
  onChange,
  placeholder,
}: {
  name: string;
  usage: string;
  provider: string;
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
}) {
  const { t } = useI18n();
  const [state, setState] = useState<TestState>("idle");
  const [error, setError] = useState<string | null>(null);

  // A previous verdict says nothing about a key that has since been edited
  useEffect(() => {
    setState("idle");
    setError(null);
  }, [value]);

  const test = () => {
    setState("testing");
    setError(null);
    invoke("test_provider_key", { provider, key: value })
      .then(() => setState("ok"))
      .catch((e) => {
        setState("failed");
        setError(String(e));
      });
  };

  return (
    <div className="settings-section">
      <div className="provider-head">
        <h2>{name}</h2>
        <span className="provider-usage">{usage}</span>
      </div>
      <div className="provider-row">
        <input
          type="password"
          className="settings-input"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
        />
        <button className="btn-secondary" onClick={test} disabled={!value || state === "testing"}>
          {state === "testing" ? t("testing") : t("test_key")}
        </button>
      </div>
      {state === "ok" && (
        <p className="model-status-badge model-status-ok">✓ {t("key_valid")}</p>
      )}
      {state === "failed" && (
        <p className="model-error">
          {t("key_invalid")}: {error}
        </p>
      )}
    </div>
  );
}

/// Apple's on-device model: nothing to configure, only a state to report.
function AppleProvider({ status }: { status: AppleStatus | null }) {
  const { t } = useI18n();
  const ready = status?.availability === "available";

  return (
    <div className="settings-section">
      <div className="provider-head">
        <h2>{t("apple_intelligence")}</h2>
        <span className="provider-usage">{t("usage_reformulation")}</span>
      </div>
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

/// Parakeet: no key, but a 640 MB model to fetch. Sitting here rather than on
/// the engine page means it can be prepared without switching engine first.
function ParakeetProvider({ onChanged }: { onChanged: () => void }) {
  const { t } = useI18n();
  const { parakeet, refresh } = useLocalProviders();
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    refresh();
    onChanged();
  }, [refresh, onChanged]);

  useEffect(() => {
    const unlistenProgress = listen<DownloadProgress>("parakeet_download_progress", (e) => {
      setProgress(e.payload);
    });
    const unlistenDone = listen("parakeet_download_done", () => {
      setProgress(null);
      reload();
    });
    const unlistenError = listen<string>("parakeet_download_error", (e) => {
      setProgress(null);
      setError(e.payload);
      reload();
    });
    const unlistenCancelled = listen("parakeet_download_cancelled", () => {
      setProgress(null);
      reload();
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenDone.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenCancelled.then((fn) => fn());
    };
  }, [reload]);

  const startDownload = () => {
    setError(null);
    invoke("download_parakeet_model").catch((e) => {
      setError(String(e));
      reload();
    });
  };

  const percent =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : 0;

  return (
    <div className="settings-section">
      <div className="provider-head">
        <h2>{t("parakeet_api")}</h2>
        <span className="provider-usage">{t("usage_transcription")}</span>
      </div>
      <p className="hint">{t("parakeet_model_hint")}</p>

      {parakeet?.downloading ? (
        <div className="model-download">
          <div className="model-status-row">
            <span className="model-status-text">
              {t("downloading_model")} ({progress ? `${progress.file_index}/${progress.file_count}` : "..."})
              {progress && progress.total > 0 && (
                <> — {formatMB(progress.downloaded)} / {formatMB(progress.total)} Mo</>
              )}
            </span>
            <button
              className="btn-secondary"
              onClick={() => invoke("cancel_parakeet_download").catch(console.error)}
            >
              {t("cancel_download")}
            </button>
          </div>
          <div className="progress-track">
            <div className="progress-fill" style={{ width: `${percent}%` }} />
          </div>
        </div>
      ) : parakeet?.downloaded ? (
        <div className="model-status-row">
          <span className="model-status-badge model-status-ok">✓ {t("model_downloaded")}</span>
          <button
            className="btn-secondary"
            onClick={() =>
              invoke("delete_parakeet_model")
                .then(reload)
                .catch((e) => setError(String(e)))
            }
          >
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

export function ProvidersPage({
  apiKey,
  setApiKey,
  mistralApiKey,
  setMistralApiKey,
  geminiApiKey,
  setGeminiApiKey,
  groqApiKey,
  setGroqApiKey,
}: ProvidersPageProps) {
  const { t } = useI18n();
  const { apple, appleSupported, refresh } = useLocalProviders();

  const both = t("usage_both");
  const apis: {
    name: string;
    provider: string;
    value: string;
    onChange: (v: string) => void;
    placeholder: string;
    usage: string;
  }[] = [
    { name: "OpenAI", provider: "openai", value: apiKey, onChange: setApiKey, placeholder: "sk-...", usage: both },
    { name: "Groq", provider: "groq", value: groqApiKey, onChange: setGroqApiKey, placeholder: "gsk_...", usage: both },
    { name: "Mistral", provider: "voxtral", value: mistralApiKey, onChange: setMistralApiKey, placeholder: "", usage: both },
    { name: "Gemini (Google)", provider: "gemini", value: geminiApiKey, onChange: setGeminiApiKey, placeholder: "", usage: both },
  ];

  return (
    <>
      <h2 className="page-title">{t("page_providers")}</h2>
      <p className="hint page-intro">{t("providers_intro")}</p>

      {/* Local first: nothing to sign up for, nothing leaves the machine */}
      <h3 className="provider-group">{t("providers_local")}</h3>
      <ParakeetProvider onChanged={refresh} />
      {appleSupported && <AppleProvider status={apple} />}

      <h3 className="provider-group">{t("providers_remote")}</h3>
      {apis.map((api) => (
        <ApiProvider key={api.provider} {...api} />
      ))}
    </>
  );
}
