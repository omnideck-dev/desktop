import { openUrl } from "@tauri-apps/plugin-opener";

/** Help/Community render in-window via iframe, per product decision — a
 * change from the original plan (desktop_tauri_rewrite.md originally called
 * for external-browser-only). Some sites (Slack's invite flow in
 * particular) send `X-Frame-Options`/`frame-ancestors` that refuse to be
 * embedded at all, and a cross-origin iframe gives JS no reliable way to
 * detect that refusal — so this always shows an "Open in browser" fallback
 * rather than trying to detect failure. */
export default function ExternalPage({ title, url }: { title: string; url: string }) {
  return (
    <div className="external-page">
      <div className="external-page__bar">
        <span className="external-page__title">{title}</span>
        <button className="ghost-button" onClick={() => openUrl(url)}>
          Open in browser
        </button>
      </div>
      <iframe
        key={url}
        src={url}
        className="external-page__frame"
        title={title}
        sandbox="allow-scripts allow-forms allow-same-origin allow-popups allow-popups-to-escape-sandbox"
      />
    </div>
  );
}
