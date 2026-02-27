import { Mic, Zap, Keyboard, Settings } from "lucide-react";
import { useI18n, type TranslationKey } from "../i18n";
import type { Page } from "../types";

interface NavItem {
  id: Page;
  labelKey: TranslationKey;
  icon: React.ReactNode;
}

const navItems: NavItem[] = [
  { id: "dictation", labelKey: "nav_dictation", icon: <Mic size={18} /> },
  { id: "engine", labelKey: "nav_engine", icon: <Zap size={18} /> },
  { id: "shortcut", labelKey: "nav_shortcut", icon: <Keyboard size={18} /> },
];

const bottomNavItem: NavItem = {
  id: "settings",
  labelKey: "nav_settings",
  icon: <Settings size={18} />,
};

interface SidebarProps {
  activePage: Page;
  onPageChange: (page: Page) => void;
}

export function Sidebar({ activePage, onPageChange }: SidebarProps) {
  const { t } = useI18n();

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h1>Dictea</h1>
      </div>

      <nav className="sidebar-nav">
        {navItems.map((item) => (
          <button
            key={item.id}
            data-page={item.id}
            className={`sidebar-item${activePage === item.id ? " active" : ""}`}
            onClick={() => onPageChange(item.id)}
          >
            <span className="item-icon">{item.icon}</span>
            {t(item.labelKey)}
          </button>
        ))}
      </nav>

      <div className="sidebar-bottom">
        <button
          data-page="settings"
          className={`sidebar-item${activePage === "settings" ? " active" : ""}`}
          onClick={() => onPageChange("settings")}
        >
          <span className="item-icon">{bottomNavItem.icon}</span>
          {t(bottomNavItem.labelKey)}
        </button>
      </div>
    </aside>
  );
}
