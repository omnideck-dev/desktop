import type { ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

export type View = "dashboard" | "settings";

// TODO(product): no real docs/Slack URL exists anywhere in this repo or the
// CLI repo yet — these are placeholders so Help/Community are wired up
// end-to-end (external open, never in-window) but not yet pointed
// anywhere real. Swap in real URLs once they exist; don't guess them.
const HELP_URL = "https://omnideck.dev/docs";
const COMMUNITY_URL = "https://omnideck.dev/community";

export default function AppShell({
  view,
  onViewChange,
  children,
}: {
  view: View;
  onViewChange: (view: View) => void;
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

        {/* Open instance tabs land here once "Open UI" is wired up
            (DESIGN.md #1/#7) — no tabs to render yet in this pass. */}

        <div className="app-nav__spacer" />

        <button className="app-nav__item" onClick={() => openUrl(HELP_URL)}>
          Help
        </button>
        <button className="app-nav__item" onClick={() => openUrl(COMMUNITY_URL)}>
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
