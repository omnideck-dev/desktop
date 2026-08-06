import type { CliVersionState } from "../hooks/useCliVersion";
import { cliErrorSummary } from "../types/cli";

export default function SettingsView({ cliVersion }: { cliVersion: CliVersionState }) {
  return (
    <div className="settings">
      <h1>Settings</h1>

      <section className="settings__section">
        <h2>Omnideck CLI</h2>
        {cliVersion.status === "ok" ? (
          <dl className="settings__facts">
            <dt>Version</dt>
            <dd>{cliVersion.info.version}</dd>
            <dt>Commit</dt>
            <dd>{cliVersion.info.commit}</dd>
            <dt>Built</dt>
            <dd>{cliVersion.info.date}</dd>
            <dt>JSON contract</dt>
            <dd>v{cliVersion.info.jsonContract}</dd>
          </dl>
        ) : cliVersion.status === "error" ? (
          <p className="settings__error">{cliErrorSummary(cliVersion.error)}</p>
        ) : (
          <p>Checking…</p>
        )}
      </section>

      <section className="settings__section">
        <h2>Advanced logging</h2>
        <p className="settings__note">Coming in a later pass — off by default.</p>
      </section>
    </div>
  );
}
