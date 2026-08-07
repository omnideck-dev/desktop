import type { ReactNode } from "react";

export type InstanceTab = { name: string; port: string };
export type View = "dashboard" | "settings" | "help" | "community" | `instance:${string}`;

export const HELP_URL = "https://www.omnideck.dev/docs/";
export const COMMUNITY_URL =
  "https://join.slack.com/t/omnideckcommunity/shared_invite/zt-43pog9zx2-i0CojjlIjyqvVSdGg4bLHg";

export function instanceView(name: string): View {
  return `instance:${name}`;
}

export default function AppShell({
  view,
  onViewChange,
  openTabs,
  onCloseTab,
  children,
}: {
  view: View;
  onViewChange: (view: View) => void;
  openTabs: InstanceTab[];
  onCloseTab: (name: string) => void;
  children: ReactNode;
}) {
  return (
    <div className="app-shell">
      <nav className="app-nav">
        <div className="app-nav__brand">
          <div className="mark" aria-hidden="true">
            <span />
            <span />
            <span />
          </div>
          <span className="app-nav__brand-text">omnideck</span>
        </div>

        <button
          className={`app-nav__item ${view === "dashboard" ? "is-active" : ""}`}
          onClick={() => onViewChange("dashboard")}
        >
          Dashboard
        </button>

        {openTabs.map((tab) => (
          <span
            key={tab.name}
            className={`app-nav__tab ${view === instanceView(tab.name) ? "is-active" : ""}`}
          >
            <button className="app-nav__tab-label" onClick={() => onViewChange(instanceView(tab.name))}>
              {tab.name}
            </button>
            <button
              className="app-nav__tab-close"
              aria-label={`Close ${tab.name} tab`}
              onClick={(e) => {
                e.stopPropagation();
                onCloseTab(tab.name);
              }}
            >
              ×
            </button>
          </span>
        ))}

        <div className="app-nav__spacer" />

        <button
          className={`app-nav__item ${view === "help" ? "is-active" : ""}`}
          onClick={() => onViewChange("help")}
        >
          Help
        </button>
        <button
          className={`app-nav__item ${view === "community" ? "is-active" : ""}`}
          onClick={() => onViewChange("community")}
        >
          Community
        </button>
        <button
          className={`app-nav__item ${view === "settings" ? "is-active" : ""}`}
          onClick={() => onViewChange("settings")}
        >
          Settings
        </button>
      </nav>

      <main className="app-content">{children}</main>
    </div>
  );
}
