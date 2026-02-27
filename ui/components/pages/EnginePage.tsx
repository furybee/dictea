import { useI18n } from "../../i18n";

interface EnginePageProps {
  apiKey: string;
  setApiKey: (v: string) => void;
  mistralApiKey: string;
  setMistralApiKey: (v: string) => void;
  geminiApiKey: string;
  setGeminiApiKey: (v: string) => void;
  sttEngine: string;
  setSttEngine: (v: string) => void;
}

export function EnginePage({
  apiKey,
  setApiKey,
  mistralApiKey,
  setMistralApiKey,
  geminiApiKey,
  setGeminiApiKey,
  sttEngine,
  setSttEngine,
}: EnginePageProps) {
  const { t } = useI18n();

  const apiKeyLabel =
    sttEngine === "voxtral"
      ? t("api_key_mistral")
      : sttEngine === "gemini"
        ? t("api_key_gemini")
        : t("api_key");

  const apiKeyHint =
    sttEngine === "voxtral"
      ? t("api_key_mistral_hint")
      : sttEngine === "gemini"
        ? t("api_key_gemini_hint")
        : t("api_key_hint");

  const currentApiKey =
    sttEngine === "voxtral"
      ? mistralApiKey
      : sttEngine === "gemini"
        ? geminiApiKey
        : apiKey;

  const setCurrentApiKey =
    sttEngine === "voxtral"
      ? setMistralApiKey
      : sttEngine === "gemini"
        ? setGeminiApiKey
        : setApiKey;

  const transcriptionModel =
    sttEngine === "voxtral"
      ? "voxtral-mini-latest"
      : sttEngine === "gemini"
        ? "gemini-2.5-flash"
        : "gpt-4o-transcribe";

  const reformulationModel =
    sttEngine === "voxtral"
      ? "mistral-small-latest"
      : sttEngine === "gemini"
        ? "gemini-2.5-flash-lite"
        : "gpt-4o-mini";

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
          <option value="voxtral">{t("voxtral_api")}</option>
          <option value="gemini">{t("gemini_api")}</option>
        </select>
      </div>

      <div className="settings-section">
        <h2>{apiKeyLabel}</h2>
        <p className="hint">{apiKeyHint}</p>
        <input
          type="password"
          className="settings-input"
          value={currentApiKey}
          onChange={(e) => setCurrentApiKey(e.target.value)}
          placeholder={sttEngine === "openai" ? "sk-..." : ""}
        />
      </div>

      <div className="settings-section">
        <h2>{t("models_used")}</h2>
        <p className="hint">{t("models_used_hint")}</p>
        <div className="models-list">
          <div className="model-item">
            <span className="model-label">{t("model_transcription")}</span>
            <code className="model-name">{transcriptionModel}</code>
          </div>
          <div className="model-item">
            <span className="model-label">{t("model_reformulation")}</span>
            <code className="model-name">{reformulationModel}</code>
          </div>
        </div>
      </div>
    </>
  );
}
