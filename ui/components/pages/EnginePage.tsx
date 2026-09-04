import { useI18n } from "../../i18n";
import { useLocalProviders } from "../../hooks/useLocalProviders";

interface EnginePageProps {
  apiKey: string;
  mistralApiKey: string;
  geminiApiKey: string;
  groqApiKey: string;
  sttEngine: string;
  setSttEngine: (v: string) => void;
  reformProvider: string;
  setReformProvider: (v: string) => void;
  onGoToProviders: () => void;
}

interface Choice {
  id: string;
  label: string;
  transcription?: string;
  reformulation?: string;
  ready: boolean;
}

export function EnginePage({
  apiKey,
  mistralApiKey,
  geminiApiKey,
  groqApiKey,
  sttEngine,
  setSttEngine,
  reformProvider,
  setReformProvider,
  onGoToProviders,
}: EnginePageProps) {
  const { t } = useI18n();
  const { parakeetReady, appleReady, appleSupported } = useLocalProviders();

  // Configured means the provider has what it needs to run: a key for the API
  // ones, a downloaded model for Parakeet, an enabled OS feature for Apple.
  const engines: Choice[] = [
    { id: "openai", label: t("openai_api"), transcription: "gpt-transcribe", reformulation: "gpt-4o-mini", ready: !!apiKey },
    { id: "groq", label: t("groq_api"), transcription: "whisper-large-v3-turbo", reformulation: "llama-3.3-70b-versatile", ready: !!groqApiKey },
    { id: "voxtral", label: t("voxtral_api"), transcription: "voxtral-mini-2602", reformulation: "mistral-small-latest", ready: !!mistralApiKey },
    { id: "gemini", label: t("gemini_api"), transcription: "gemini-3.5-transcribe", reformulation: "gemini-2.5-flash-lite", ready: !!geminiApiKey },
    { id: "parakeet", label: t("parakeet_api"), transcription: "parakeet-tdt-0.6b-v3 (local)", ready: parakeetReady },
  ];

  const providers: Choice[] = [
    { id: "auto", label: t("reformulation_auto"), ready: true },
    ...(appleSupported
      ? [
          {
            id: "apple",
            label: t("apple_intelligence"),
            reformulation: "apple on-device (local)",
            ready: appleReady,
          },
        ]
      : []),
    ...engines.filter((e) => e.id !== "parakeet"),
  ];

  const current = engines.find((e) => e.id === sttEngine) ?? engines[0];
  // Mirrors resolve_reformulation_provider on the Rust side
  const resolvedProvider =
    reformProvider === "auto"
      ? sttEngine === "parakeet"
        ? "openai"
        : sttEngine
      : reformProvider;
  const reform = providers.find((p) => p.id === resolvedProvider) ?? providers[0];

  // Unconfigured options stay listed rather than disappearing: an option that
  // vanishes cannot be discovered, and makes the app look broken.
  const option = (c: Choice) => (
    <option key={c.id} value={c.id} disabled={!c.ready}>
      {c.label}
      {c.ready ? "" : ` — ${t("not_configured")}`}
    </option>
  );

  const missing = !current.ready || !reform.ready;

  return (
    <>
      <h2 className="page-title">{t("page_engine")}</h2>

      {missing && (
        <div className="alert-banner">
          <div className="alert-banner-text">
            <strong>{t("provider_missing_title")}</strong>
            <p>{t("provider_missing_body")}</p>
          </div>
          <div className="alert-banner-actions">
            <button className="btn-primary" onClick={onGoToProviders}>
              {t("open_providers")}
            </button>
          </div>
        </div>
      )}

      <div className="settings-section">
        <h2>{t("stt_engine")}</h2>
        <p className="hint">{t("stt_engine_hint")}</p>
        <select
          className="settings-select"
          value={sttEngine}
          onChange={(e) => setSttEngine(e.target.value)}
        >
          {engines.map(option)}
        </select>
      </div>

      <div className="settings-section">
        <h2>{t("reformulation_provider")}</h2>
        <p className="hint">{t("reformulation_provider_hint")}</p>
        <select
          className="settings-select"
          value={reformProvider}
          onChange={(e) => setReformProvider(e.target.value)}
        >
          {providers.map(option)}
        </select>
      </div>

      <div className="settings-section">
        <h2>{t("models_used")}</h2>
        <div className="models-list">
          <div className="model-item">
            <span className="model-label">{t("model_transcription")}</span>
            <code className="model-name">{current.transcription}</code>
          </div>
          <div className="model-item">
            <span className="model-label">{t("model_reformulation")}</span>
            <code className="model-name">{reform.reformulation ?? "—"}</code>
          </div>
        </div>
      </div>
    </>
  );
}
