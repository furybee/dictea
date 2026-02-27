import { useI18n, type AppLang } from "../../i18n";
import { useUpdater } from "../../hooks/useUpdater";

export function SettingsPage() {
  const { t, lang, setLang } = useI18n();
  const { status, version, checkAndDownload, dismiss } = useUpdater();

  return (
    <>
      <h2 className="page-title">{t("page_settings")}</h2>

      {status === "available" && (
        <div className="update-banner">
          <span>
            {t("update_available")} — {t("update_version")} {version}
          </span>
          <div className="update-banner-actions">
            <button className="btn-primary" onClick={checkAndDownload}>
              {t("update_install")}
            </button>
            <button className="btn-secondary" onClick={dismiss}>
              {t("update_later")}
            </button>
          </div>
        </div>
      )}

      {status === "downloading" && (
        <div className="update-banner">
          <span>{t("update_downloading")}</span>
        </div>
      )}

      {status === "done" && (
        <div className="update-banner">
          <span>{t("update_restart")}</span>
        </div>
      )}

      <div className="settings-section">
        <h2>{t("app_language")}</h2>
        <p className="hint">{t("app_language_hint")}</p>
        <select
          className="settings-select"
          value={lang}
          onChange={(e) => setLang(e.target.value as AppLang)}
        >
          <option value="fr">Francais</option>
          <option value="en">English</option>
        </select>
      </div>

      <div className="settings-section">
        <div className="about-content">
          <p>
            <strong>Dictea</strong> v0.2
          </p>
          <p>{t("about_desc")}</p>
          <br />
          <p>{t("about_engines")}</p>
          <p>{t("about_features")}</p>
        </div>
      </div>
    </>
  );
}
