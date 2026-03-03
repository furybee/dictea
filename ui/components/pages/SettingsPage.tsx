import { useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useState } from "react";
import { useI18n, type AppLang } from "../../i18n";
import { useUpdater } from "../../hooks/useUpdater";
import { useAudioDevices } from "../../hooks/useAudioDevices";

interface SettingsPageProps {
  audioDevice: string;
  setAudioDevice: (v: string) => void;
}

export function SettingsPage({ audioDevice, setAudioDevice }: SettingsPageProps) {
  const { t, lang, setLang } = useI18n();
  const { status, version, checkAndDownload, dismiss } = useUpdater();
  const { devices, level, refreshDevices, startPreview, stopPreview } =
    useAudioDevices();
  const [appVersion, setAppVersion] = useState("");

  useEffect(() => {
    getVersion().then(setAppVersion);
  }, []);

  useEffect(() => {
    refreshDevices();
    startPreview(audioDevice);
    return () => {
      stopPreview();
    };
  }, []);

  useEffect(() => {
    startPreview(audioDevice);
  }, [audioDevice]);

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
        <h2>{t("audio_device")}</h2>
        <p className="hint">{t("audio_device_hint")}</p>
        <div className="device-select-row">
          <select
            className="settings-select"
            value={audioDevice}
            onChange={(e) => setAudioDevice(e.target.value)}
          >
            <option value="">{t("audio_device_default")}</option>
            {devices.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
          <div className="volume-meter">
            <div
              className="volume-meter-fill"
              style={{ width: `${level * 100}%` }}
            />
          </div>
        </div>
      </div>

      <div className="settings-section">
        <div className="about-content">
          <p>
            <strong>Dictea</strong> v{appVersion}
          </p>
        </div>
      </div>
    </>
  );
}
