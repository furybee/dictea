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

  const current = engineConfig[sttEngine] || engineConfig.openai;

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
        </select>
      </div>

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
  );
}
