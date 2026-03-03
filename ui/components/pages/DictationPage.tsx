import { useI18n } from "../../i18n";
import { OUTPUT_LANGUAGES } from "../../types";

interface DictationPageProps {
  outputLanguage: string;
  setOutputLanguage: (v: string) => void;
  reformulate: boolean;
  setReformulate: (v: boolean) => void;
}

export function DictationPage({
  outputLanguage,
  setOutputLanguage,
  reformulate,
  setReformulate,
}: DictationPageProps) {
  const { t } = useI18n();

  return (
    <>
      <h2 className="page-title">{t("page_dictation")}</h2>

      <div className="settings-section">
        <h2>{t("output_language")}</h2>
        <p className="hint">
          {outputLanguage === "auto"
            ? t("output_language_hint_auto")
            : t("output_language_hint_translate")}
        </p>
        <select
          className="settings-select"
          value={outputLanguage}
          onChange={(e) => setOutputLanguage(e.target.value)}
        >
          {OUTPUT_LANGUAGES.map((lang) => (
            <option key={lang.code} value={lang.code}>
              {lang.labelKey ? t(lang.labelKey) : lang.label}
            </option>
          ))}
        </select>
      </div>

      <div className="settings-section">
        <label className="toggle-row">
          <div className="toggle-row-text">
            <h2>{t("reformulate")}</h2>
            <p className="hint">{t("reformulate_hint")}</p>
          </div>
          <div className="toggle-switch">
            <input
              type="checkbox"
              checked={reformulate}
              onChange={(e) => setReformulate(e.target.checked)}
            />
            <span className="toggle-slider" />
          </div>
        </label>
      </div>
    </>
  );
}
