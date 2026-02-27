import { useI18n } from "../../i18n";

export function ShortcutPage() {
  const { t } = useI18n();

  return (
    <>
      <h2 className="page-title">{t("page_shortcut")}</h2>

      <div className="settings-section">
        <h2>{t("global_shortcut")}</h2>
        <p className="hint">{t("shortcut_hint")}</p>
        <div className="shortcut-display">
          <kbd>Cmd</kbd>
          <span className="kbd-plus">+</span>
          <kbd>Shift</kbd>
          <span className="kbd-plus">+</span>
          <kbd>Space</kbd>
        </div>
      </div>

      <div className="settings-section">
        <h2>{t("cancel_shortcut")}</h2>
        <p className="hint">{t("cancel_shortcut_hint")}</p>
        <div className="shortcut-display">
          <kbd>Cmd</kbd>
          <span className="kbd-plus">+</span>
          <kbd>Shift</kbd>
          <span className="kbd-plus">+</span>
          <kbd>C</kbd>
        </div>
      </div>
    </>
  );
}
